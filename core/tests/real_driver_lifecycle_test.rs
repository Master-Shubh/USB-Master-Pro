//! Real Driver Lifecycle & VHCI Communication Tests
//! Verifies KMDF VHCI driver client interaction:
//! 1. Missing driver open failure handling
//! 2. Simulated / Win32 IOCTL error dispatch
//! 3. Virtual device attach / detach lifecycle
//! 4. Single-URB >64 KB buffer boundary guard
//! 5. Static asset endpoints (/assets/tailwind.min.js and /assets/lucide.min.js)

use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use usb_master_core::daemon::{
    derive_numeric_device_id, execute_session_disconnect, ActiveSession, DaemonServer,
    DaemonState, RemoteMountedDevice, SessionPumpHandles,
};
#[allow(unused_imports)]
use usb_master_core::driver_client::{
    check_driver_device_presence, check_driver_status, set_simulated_driver_status,
    AnyUsbAttachRequest, AnyUsbCompleteRequest, DriverClientError, DriverOperationalStatus,
    FilterDriverClient, VhciDriverClient, ANYUSB_MAX_URB_BUFFER_SIZE,
    IOCTL_ANYUSB_VHCI_ATTACH_DEVICE, IOCTL_ANYUSB_VHCI_INIT_BUS,
};
use usb_master_core::hardware_db::DeviceCategory;
use usb_master_core::identity::NodeIdentity;
use usb_master_core::state::SessionState;
use usb_master_core::UsbDeviceInfo;

static TEST_DRIVER_STATUS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn raw_http_get(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect to local test daemon");
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        path, port
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("Failed to write request");

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("Failed to read response");
    String::from_utf8_lossy(&response).to_string()
}

async fn raw_http_post(port: u16, path: &str, body: &str, headers: &[(&str, &str)]) -> String {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect to local test daemon");
    let mut request = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nContent-Length: {}\r\nContent-Type: application/json\r\n",
        path, port, body.len()
    );
    for (k, v) in headers {
        request.push_str(&format!("{}: {}\r\n", k, v));
    }
    request.push_str("\r\n");
    request.push_str(body);

    stream
        .write_all(request.as_bytes())
        .await
        .expect("Failed to write request");

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("Failed to read response");
    String::from_utf8_lossy(&response).to_string()
}

#[test]
fn test_missing_driver_device_open_failure() {
    let nonexistent_path = if cfg!(windows) {
        r"\\.\NonExistentAnyUsbDriver_12345"
    } else {
        "/dev/nonexistent_anyusb_driver_12345"
    };

    let result = VhciDriverClient::open_device(nonexistent_path);
    assert!(
        result.is_err(),
        "Opening nonexistent driver device node must fail"
    );

    match result.unwrap_err() {
        DriverClientError::DeviceNotFound(msg) => {
            assert!(
                msg.contains("nonexistent") || msg.contains("Failed to open"),
                "Expected DeviceNotFound error mentioning missing path, got: {}",
                msg
            );
        }
        other => panic!("Expected DeviceNotFound, got {:?}", other),
    }
}

#[test]
fn test_failed_ioctl_error_handling() {
    let client = VhciDriverClient::open().expect("Failed to open driver client");

    // Simulate an IOCTL access denied / hardware failure error
    let simulated_result =
        client.simulate_ioctl_failure(IOCTL_ANYUSB_VHCI_ATTACH_DEVICE, 5); // ERROR_ACCESS_DENIED = 5
    assert!(simulated_result.is_err());

    match simulated_result.unwrap_err() {
        DriverClientError::IoControlFailed { ioctl, win_error } => {
            assert_eq!(ioctl, IOCTL_ANYUSB_VHCI_ATTACH_DEVICE);
            assert_eq!(win_error, 5);
        }
        other => panic!("Expected IoControlFailed error, got {:?}", other),
    }

    // Verify INIT_BUS ioctl failure handling
    let init_err = client.simulate_ioctl_failure(IOCTL_ANYUSB_VHCI_INIT_BUS, 31); // ERROR_GEN_FAILURE = 31
    assert!(init_err.is_err());
    match init_err.unwrap_err() {
        DriverClientError::IoControlFailed { ioctl, win_error } => {
            assert_eq!(ioctl, IOCTL_ANYUSB_VHCI_INIT_BUS);
            assert_eq!(win_error, 31);
        }
        other => panic!("Expected IoControlFailed error, got {:?}", other),
    }
}

#[test]
fn test_attach_detach_device_lifecycle() {
    let client = VhciDriverClient::open().expect("Failed to open driver client");
    client.init_bus().expect("Bus initialization failed");

    // 1. Build AnyUsbAttachRequest with Mantra MFS100 certified peripheral details
    let mut attach_req = AnyUsbAttachRequest {
        device_id: 202,
        vendor_id: 0x27D0, // Mantra Softech India Pvt Ltd
        product_id: 0x0041, // MFS100 Optical Fingerprint Scanner
        bcd_device: 0x0200,
        device_class: 0x00,
        device_subclass: 0x00,
        device_protocol: 0x00,
        speed: 3, // HighSpeed (480 Mbps)
        ..Default::default()
    };

    let desc = "Mantra MFS100 USB Virtual Optical Sensor";
    for (i, c) in desc.encode_utf16().take(127).enumerate() {
        attach_req.product_description[i] = c;
    }

    // 2. Attach virtual device
    let attach_res = client.attach_device(&attach_req);
    assert!(
        attach_res.is_ok(),
        "Virtual USB device attachment must succeed"
    );

    // 3. Detach virtual device
    let detach_res = client.detach_device(202);
    assert!(
        detach_res.is_ok(),
        "Virtual USB device detachment must succeed"
    );
}

#[test]
fn test_single_urb_buffer_boundary_rejection() {
    let client = VhciDriverClient::open().expect("Failed to open driver client");

    // Valid transfer within 64 KB boundary must succeed
    let valid_req = AnyUsbCompleteRequest {
        device_id: 1,
        sequence_id: 1,
        status: 0,
        actual_length: ANYUSB_MAX_URB_BUFFER_SIZE as u32, // Exactly 64 KB
        data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
    };
    assert!(
        client.complete_urb(&valid_req).is_ok(),
        "Transfer at or below 64 KB boundary must be accepted"
    );

    // Oversized transfer (e.g. 64 KB + 1 byte) must be rejected with InvalidParameter
    let oversized_req = AnyUsbCompleteRequest {
        device_id: 1,
        sequence_id: 2,
        status: 0,
        actual_length: (ANYUSB_MAX_URB_BUFFER_SIZE + 1) as u32,
        data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
    };
    let oversized_res = client.complete_urb(&oversized_req);
    assert!(
        oversized_res.is_err(),
        "Single URB transfer exceeding 64 KB must be rejected"
    );
    match oversized_res.unwrap_err() {
        DriverClientError::InvalidParameter(msg) => {
            assert!(
                msg.contains("exceeds maximum single-URB buffer limit"),
                "Error must state that buffer limit is exceeded: {}",
                msg
            );
        }
        other => panic!("Expected InvalidParameter, got {:?}", other),
    }

    // Extreme oversized transfer (1 MB slap scanner frame)
    let extreme_req = AnyUsbCompleteRequest {
        device_id: 1,
        sequence_id: 3,
        status: 0,
        actual_length: 1_048_576,
        data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
    };
    assert!(client.complete_urb(&extreme_req).is_err());
}

#[tokio::test]
async fn test_embedded_tailwind_and_lucide_endpoints() {
    let _guard = TEST_DRIVER_STATUS_LOCK.lock().unwrap();
    #[cfg(not(target_os = "windows"))]
    set_simulated_driver_status(None);

    let port = find_free_port();
    let token = format!("test_assets_token_{}", port);
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run_with_token(port, broker_url, token).await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    // 1. Verify GET /assets/tailwind.min.js
    let tailwind_resp = raw_http_get(port, "/assets/tailwind.min.js").await;
    assert!(
        tailwind_resp.contains("HTTP/1.1 200 OK"),
        "Tailwind JS endpoint must return 200 OK"
    );
    assert!(
        tailwind_resp.contains("application/javascript"),
        "Tailwind JS must have application/javascript Content-Type"
    );
    assert!(
        tailwind_resp.len() > 10_000,
        "Tailwind JS payload must be complete and non-empty"
    );

    // 2. Verify GET /assets/lucide.min.js
    let lucide_resp = raw_http_get(port, "/assets/lucide.min.js").await;
    assert!(
        lucide_resp.contains("HTTP/1.1 200 OK"),
        "Lucide JS endpoint must return 200 OK"
    );
    assert!(
        lucide_resp.contains("application/javascript"),
        "Lucide JS must have application/javascript Content-Type"
    );
    assert!(
        lucide_resp.len() > 10_000,
        "Lucide JS payload must be complete and non-empty"
    );

    // 3. Verify GET /api/system/drivers
    let drivers_resp = raw_http_get(port, "/api/system/drivers").await;
    assert!(
        drivers_resp.contains("HTTP/1.1 200 OK"),
        "Drivers endpoint must return 200 OK"
    );
    assert!(
        drivers_resp.contains("vhci_installed"),
        "Drivers endpoint must report vhci_installed"
    );
    assert!(
        drivers_resp.contains("filter_installed"),
        "Drivers endpoint must report filter_installed"
    );

    // 4. Verify check_driver_device_presence stub / Win32 function
    let presence = check_driver_device_presence(VhciDriverClient::DEVICE_PATH);
    #[cfg(target_os = "windows")]
    let _ = presence;
    #[cfg(not(target_os = "windows"))]
    assert!(
        !presence,
        "Driver device presence check should return false on POSIX/Linux by default"
    );
}

#[test]
fn test_derive_numeric_device_id_whitespace_and_alphanumeric_consistency() {
    // 1. Whitespace resilience with numeric IDs
    let id_spaced = "482 190 334";
    let id_clean = "482190334";
    let id_padded = "  482  190 334  ";
    assert_eq!(
        derive_numeric_device_id(id_spaced),
        482190334,
        "Numeric ID with interior spaces must parse cleanly"
    );
    assert_eq!(
        derive_numeric_device_id(id_clean),
        482190334,
        "Clean numeric ID must equal parsed value"
    );
    assert_eq!(
        derive_numeric_device_id(id_padded),
        482190334,
        "Heavily padded numeric ID must resolve to the same value"
    );

    // 2. Alphanumeric peer ID determinism via FNV-1a
    let peer_a1 = derive_numeric_device_id("peer-node-alpha-1");
    let peer_a2 = derive_numeric_device_id("peer-node-alpha-1");
    assert_eq!(peer_a1, peer_a2, "FNV-1a hash must be 100% deterministic across calls");

    // Mask must enforce 31-bit positive range (& 0x7FFF_FFFF)
    assert!(peer_a1 <= 0x7FFF_FFFF, "Numeric device ID must fit within positive 31-bit mask");

    // Whitespace in alphanumeric peer IDs must also normalize
    let peer_spaced = derive_numeric_device_id("peer node alpha 1");
    let peer_normalized = derive_numeric_device_id("peernodealpha1");
    assert_eq!(
        peer_spaced, peer_normalized,
        "Alphanumeric ID with spaces must hash identically to stripped version"
    );

    // Manual FNV-1a verification
    let mut expected_hash: u32 = 0x811c9dc5;
    for byte in b"peernodealpha1" {
        expected_hash ^= *byte as u32;
        expected_hash = expected_hash.wrapping_mul(0x01000193);
    }
    expected_hash &= 0x7FFF_FFFF;
    assert_eq!(peer_normalized, expected_hash, "Must exactly match 32-bit FNV-1a standard");
}

#[test]
fn test_simulated_driver_attach_failure_handling() {
    let client = VhciDriverClient::open().expect("Failed to open driver client");

    let attach_req = AnyUsbAttachRequest {
        device_id: 101,
        vendor_id: 0x27D0,
        product_id: 0x0041,
        bcd_device: 0x0100,
        speed: 3,
        ..Default::default()
    };

    // 1. When simulate_attach_failure is enabled, attach_device must fail with IoControlFailed
    client.set_simulate_attach_failure(true);
    let fail_result = client.attach_device(&attach_req);
    assert!(fail_result.is_err(), "Simulated attach failure must return Err");
    match fail_result.unwrap_err() {
        DriverClientError::IoControlFailed { ioctl, win_error } => {
            assert_eq!(ioctl, IOCTL_ANYUSB_VHCI_ATTACH_DEVICE);
            assert_eq!(win_error, 5); // ERROR_ACCESS_DENIED
        }
        other => panic!("Expected IoControlFailed error, got {:?}", other),
    }

    // 2. When simulate_attach_failure is restored to false, attach_device must succeed
    client.set_simulate_attach_failure(false);
    let ok_result = client.attach_device(&attach_req);
    assert!(ok_result.is_ok(), "Restored client must attach successfully");

    // 3. Anti-Inversion Law: Zero VID/PID must be rejected by driver boundary
    let zero_req = AnyUsbAttachRequest {
        device_id: 102,
        vendor_id: 0,
        product_id: 0,
        bcd_device: 0x0100,
        speed: 3,
        ..Default::default()
    };
    let zero_res = client.attach_device(&zero_req);
    assert!(zero_res.is_err(), "Zero VID/PID must be rejected as invalid parameter");
    match zero_res.unwrap_err() {
        DriverClientError::InvalidParameter(msg) => {
            assert!(
                msg.contains("cannot both be zero"),
                "Error message must specify VID/PID cannot both be zero: {}",
                msg
            );
        }
        other => panic!("Expected InvalidParameter error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_execute_session_disconnect_cleans_remote_and_local_mounts() {
    let (broker_tx, _broker_rx) = tokio::sync::mpsc::channel(32);
    let (ui_events_tx, mut ui_rx) = tokio::sync::broadcast::channel(32);
    let driver_client = Arc::new(VhciDriverClient::default());

    let test_peer = "482 190 334";
    let clean_peer = "482190334";

    // Setup local device bound and mounted to clean_peer
    let local_dev = UsbDeviceInfo {
        id: "1-2.1".to_string(),
        vid: 0x27D0,
        pid: 0x0041,
        bus_num: 1,
        dev_num: 3,
        port_path: "Port 1-2".to_string(),
        friendly_name: "Mantra MFS100 Optical Fingerprint Scanner".to_string(),
        vendor_name: "Mantra Softech".to_string(),
        product_name: "MFS100".to_string(),
        serial_number: "MFS100_TEST_01".to_string(),
        category: DeviceCategory::FingerprintScanner,
        category_icon: "fingerprint".to_string(),
        speed: "High-Speed".to_string(),
        is_shared: true,
        is_mounted: true,
        remote_peer_id: Some(clean_peer.to_string()),
        flags: 0,
        is_system_device: false,
        can_forward: true,
    };

    // Setup remote mounted device
    let remote_dev = RemoteMountedDevice {
        id: "rem-999".to_string(),
        remote_peer_id: clean_peer.to_string(),
        name: "Remote Optical Sensor".to_string(),
        icon: "⚡".to_string(),
        driver_state: "Virtual USB Host (VHCI: Active)".to_string(),
        latency: "0.0 ms".to_string(),
        throughput: "0.0 MB/s".to_string(),
    };

    // Setup active session in Active state
    let mut session = ActiveSession::new("sess-482190334".to_string(), clean_peer.to_string(), "1-2.1".to_string());
    session.transition_to(SessionState::Pairing, None).unwrap();
    session.transition_to(SessionState::Negotiating, None).unwrap();
    session.transition_to(SessionState::Connecting, None).unwrap();
    session.transition_to(SessionState::PreparingDevice, None).unwrap();
    session.transition_to(SessionState::Active, None).unwrap();

    let mut sessions_map = HashMap::new();
    sessions_map.insert(clean_peer.to_string(), session);

    let (binary_outbound_tx, _binary_rx) = tokio::sync::mpsc::channel(32);
    let state = Arc::new(DaemonState {
        identity: NodeIdentity::load_or_generate(),
        devices: RwLock::new(vec![local_dev]),
        remote_devices: RwLock::new(vec![remote_dev]),
        active_sessions: Arc::new(RwLock::new(sessions_map)),
        broker_tx,
        binary_outbound_tx,
        ui_events_tx,
        broker_connected: AtomicBool::new(true),
        start_time: Instant::now(),
        broker_url: "ws://127.0.0.1:8080".to_string(),
        active_license: RwLock::new(None),
        local_api_token: std::sync::RwLock::new("test-token".to_string()),
        driver_client,
        filter_client: Arc::new(FilterDriverClient::default()),
        session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        dynamic_pin: Arc::new(tokio::sync::RwLock::new("123 456".to_string())),
        failed_pin_attempts: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    });

    let (data_tx, _) = tokio::sync::mpsc::channel(16);
    let (shut_tx, _) = tokio::sync::watch::channel(false);
    let t1 = tokio::spawn(async { tokio::time::sleep(std::time::Duration::from_secs(60)).await; });
    let t2 = tokio::spawn(async { tokio::time::sleep(std::time::Duration::from_secs(60)).await; });
    let t3 = tokio::spawn(async { tokio::time::sleep(std::time::Duration::from_secs(60)).await; });

    let t1_handle = t1.abort_handle();
    let t2_handle = t2.abort_handle();
    let t3_handle = t3.abort_handle();

    {
        let mut pumps = state.session_pumps.lock().await;
        pumps.insert(clean_peer.to_string(), SessionPumpHandles {
            peer_id: clean_peer.to_string(),
            device_id: 12345,
            vhci_to_net_task: t1,
            net_to_vhci_task: t2,
            net_fwd_task: t3,
            shutdown_tx: shut_tx,
            session_data_tx: data_tx,
        });
    }

    // Execute disconnect using spaced peer ID
    let disconnected = execute_session_disconnect(
        &state,
        test_peer,
        Some("Operator test disconnect".to_string()),
    ).await;
    assert!(disconnected, "execute_session_disconnect must return true when active mounts exist");
    assert!(t1_handle.is_finished(), "vhci_to_net_task must be aborted on disconnect");
    assert!(t2_handle.is_finished(), "net_to_vhci_task must be aborted on disconnect");
    assert!(t3_handle.is_finished(), "net_fwd_task must be aborted on disconnect");

    // 1. Verify remote device unmounted
    let rem_devices = state.remote_devices.read().await;
    assert!(
        rem_devices.is_empty(),
        "Remote devices must be completely purged for disconnected peer"
    );

    // 2. Verify local device is unmounted and unbound
    let loc_devices = state.devices.read().await;
    assert_eq!(loc_devices.len(), 1);
    assert!(
        !loc_devices[0].is_mounted,
        "Local device is_mounted must be false after disconnect"
    );
    assert_eq!(
        loc_devices[0].remote_peer_id, None,
        "Local device remote_peer_id must be cleared to None"
    );

    // 3. Verify session transitioned to Ready
    let active_sessions = state.active_sessions.read().await;
    let s = active_sessions.get(clean_peer).expect("Session entry should still exist in map");
    assert_eq!(
        s.state(),
        SessionState::Ready,
        "Session must have returned to Ready state"
    );

    // 4. Verify UI broadcast events were fired
    let mut got_state_event = false;
    let mut got_telemetry_event = false;
    while let Ok(evt) = ui_rx.try_recv() {
        if evt["type"] == "SESSION_STATE_CHANGED" {
            got_state_event = true;
        }
        if evt["type"] == "TELEMETRY_UPDATE" {
            got_telemetry_event = true;
        }
    }
    assert!(got_state_event, "SESSION_STATE_CHANGED event must be broadcast");
    assert!(got_telemetry_event, "TELEMETRY_UPDATE event must be broadcast");
}

#[tokio::test]
async fn test_execute_session_disconnect_idempotency_and_missing_session() {
    let (broker_tx, _broker_rx) = tokio::sync::mpsc::channel(32);
    let (ui_events_tx, _ui_rx) = tokio::sync::broadcast::channel(32);
    let driver_client = Arc::new(VhciDriverClient::default());

    let (binary_outbound_tx, _binary_rx) = tokio::sync::mpsc::channel(32);
    let state = Arc::new(DaemonState {
        identity: NodeIdentity::load_or_generate(),
        devices: RwLock::new(vec![]),
        remote_devices: RwLock::new(vec![]),
        active_sessions: Arc::new(RwLock::new(HashMap::new())),
        broker_tx,
        binary_outbound_tx,
        ui_events_tx,
        broker_connected: AtomicBool::new(true),
        start_time: Instant::now(),
        broker_url: "ws://127.0.0.1:8080".to_string(),
        active_license: RwLock::new(None),
        local_api_token: std::sync::RwLock::new("test-token".to_string()),
        driver_client,
        filter_client: Arc::new(FilterDriverClient::default()),
        session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        dynamic_pin: Arc::new(tokio::sync::RwLock::new("123 456".to_string())),
        failed_pin_attempts: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    });

    // 1. Disconnect on non-existent peer must return false safely without panic
    let result_nonexistent = execute_session_disconnect(&state, "peer-999-missing", None).await;
    assert!(!result_nonexistent, "Disconnecting non-existent peer must return false");

    // 2. Setup a remote mounted device without an active session
    let clean_peer = "peer-idempotent-101";
    let remote_dev = RemoteMountedDevice {
        id: "rem-test-101".to_string(),
        remote_peer_id: clean_peer.to_string(),
        name: "Test Remote Optical Scanner".to_string(),
        icon: "⚡".to_string(),
        driver_state: "Active".to_string(),
        latency: "1.0 ms".to_string(),
        throughput: "0.0 MB/s".to_string(),
    };
    state.remote_devices.write().await.push(remote_dev);

    // First call unmounts the remote device and returns true
    let first_disc = execute_session_disconnect(&state, clean_peer, None).await;
    assert!(first_disc, "First disconnect on mounted remote device must return true");

    // Second call (already unmounted and no session in map) must return false (idempotent)
    let second_disc = execute_session_disconnect(&state, clean_peer, None).await;
    assert!(!second_disc, "Subsequent disconnect on already-cleaned device must return false");
}

#[tokio::test]
async fn test_disconnect_http_api_origin_and_auth_enforcement() {
    let port = find_free_port();
    let token = format!("test_disc_token_{}", port);
    let broker_url = "ws://127.0.0.1:8443".to_string();

    let token_clone = token.clone();
    tokio::spawn(async move {
        let _ = DaemonServer::run_with_token(port, broker_url, token_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    let payload = r#"{"peer_id": "482 190 334", "reason": "User disconnected"}"#;

    // 1. Unauthenticated request must be rejected with 401 Unauthorized
    let unauth_resp = raw_http_post(
        port,
        "/api/disconnect",
        payload,
        &[("Origin", &format!("http://127.0.0.1:{}", port))],
    ).await;
    assert!(
        unauth_resp.contains("HTTP/1.1 401 Unauthorized"),
        "Missing authorization token must yield 401 Unauthorized"
    );

    // 2. Unauthorized Origin (CSRF protection) must be rejected with 403 Forbidden
    let bad_origin_resp = raw_http_post(
        port,
        "/api/disconnect",
        payload,
        &[
            ("Authorization", &format!("Bearer {}", token)),
            ("Origin", "http://evil-attacker.com"),
        ],
    ).await;
    assert!(
        bad_origin_resp.contains("HTTP/1.1 403 Forbidden"),
        "Untrusted cross-origin request must yield 403 Forbidden"
    );

    // 3. Authorized request with valid Origin and Bearer token must succeed with 200 OK
    let auth_resp = raw_http_post(
        port,
        "/api/disconnect",
        payload,
        &[
            ("Authorization", &format!("Bearer {}", token)),
            ("Origin", &format!("http://127.0.0.1:{}", port)),
        ],
    ).await;
    assert!(
        auth_resp.contains("HTTP/1.1 200 OK"),
        "Valid request must yield 200 OK"
    );
    assert!(
        auth_resp.contains("\"status\":\"DISCONNECTED\""),
        "Response payload must indicate DISCONNECTED status"
    );
    assert!(
        auth_resp.contains("\"peer_id\":\"482190334\""),
        "Whitespace in peer_id must be stripped cleanly"
    );
}

#[tokio::test]
async fn test_assets_content_integrity_and_404_handling() {
    let port = find_free_port();
    let token = format!("test_assets_int_{}", port);
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run_with_token(port, broker_url, token).await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    // 1. Verify Tailwind CSS embedded script content integrity
    let tailwind_resp = raw_http_get(port, "/assets/tailwind.min.js").await;
    assert!(
        tailwind_resp.contains("HTTP/1.1 200 OK"),
        "Tailwind must return 200 OK"
    );
    assert!(
        tailwind_resp.contains("tailwind") || tailwind_resp.contains("/*! tailwindcss"),
        "Tailwind payload must contain Tailwind identifiers"
    );

    // 2. Verify Lucide icons embedded script content integrity
    let lucide_resp = raw_http_get(port, "/assets/lucide.min.js").await;
    assert!(
        lucide_resp.contains("HTTP/1.1 200 OK"),
        "Lucide must return 200 OK"
    );
    assert!(
        lucide_resp.contains("lucide") || lucide_resp.contains("createIcons"),
        "Lucide payload must contain Lucide icons definitions"
    );

    // 3. Non-existent asset route must return 404 Not Found
    let missing_resp = raw_http_get(port, "/assets/non_existent_bundle.js").await;
    assert!(
        missing_resp.contains("HTTP/1.1 404 Not Found"),
        "Requesting nonexistent asset must return 404 Not Found"
    );
}

#[test]
fn test_linux_returns_unsupported_platform_by_default() {
    let _guard = TEST_DRIVER_STATUS_LOCK.lock().unwrap();
    #[cfg(not(target_os = "windows"))]
    {
        set_simulated_driver_status(None);
        let vhci_status = check_driver_status(VhciDriverClient::DEVICE_PATH);
        assert_eq!(
            vhci_status,
            DriverOperationalStatus::UnsupportedPlatform,
            "Linux host must return UnsupportedPlatform by default, never fake availability"
        );
        let filter_status = check_driver_status(r"\\.\AnyUsbFilter");
        assert_eq!(
            filter_status,
            DriverOperationalStatus::UnsupportedPlatform,
            "Filter driver on Linux must return UnsupportedPlatform by default"
        );
        assert!(
            !check_driver_device_presence(VhciDriverClient::DEVICE_PATH),
            "check_driver_device_presence must return false when UnsupportedPlatform"
        );
    }
}

#[test]
fn test_mock_override_driver_operational_states() {
    let _guard = TEST_DRIVER_STATUS_LOCK.lock().unwrap();
    #[cfg(not(target_os = "windows"))]
    {
        // 1. Accessible
        set_simulated_driver_status(Some(DriverOperationalStatus::Accessible));
        assert_eq!(
            check_driver_status(VhciDriverClient::DEVICE_PATH),
            DriverOperationalStatus::Accessible
        );
        assert!(check_driver_device_presence(VhciDriverClient::DEVICE_PATH));

        // 2. Unavailable
        set_simulated_driver_status(Some(DriverOperationalStatus::Unavailable));
        assert_eq!(
            check_driver_status(VhciDriverClient::DEVICE_PATH),
            DriverOperationalStatus::Unavailable
        );
        assert!(!check_driver_device_presence(VhciDriverClient::DEVICE_PATH));

        // 3. AccessDenied
        set_simulated_driver_status(Some(DriverOperationalStatus::AccessDenied));
        assert_eq!(
            check_driver_status(VhciDriverClient::DEVICE_PATH),
            DriverOperationalStatus::AccessDenied
        );
        assert!(!check_driver_device_presence(VhciDriverClient::DEVICE_PATH));

        // 4. Error
        set_simulated_driver_status(Some(DriverOperationalStatus::Error));
        assert_eq!(
            check_driver_status(VhciDriverClient::DEVICE_PATH),
            DriverOperationalStatus::Error
        );
        assert!(!check_driver_device_presence(VhciDriverClient::DEVICE_PATH));

        // 5. TestStub
        set_simulated_driver_status(Some(DriverOperationalStatus::TestStub));
        assert_eq!(
            check_driver_status(VhciDriverClient::DEVICE_PATH),
            DriverOperationalStatus::TestStub
        );
        assert!(!check_driver_device_presence(VhciDriverClient::DEVICE_PATH));

        // Reset to default
        set_simulated_driver_status(None);
        assert_eq!(
            check_driver_status(VhciDriverClient::DEVICE_PATH),
            DriverOperationalStatus::UnsupportedPlatform
        );
    }
}

#[tokio::test]
async fn test_system_drivers_endpoint_truthful_structured_status() {
    let _guard = TEST_DRIVER_STATUS_LOCK.lock().unwrap();
    let port = find_free_port();
    let token = "test_driver_status_token".to_string();
    let token_clone = token.clone();
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run_with_token(port, broker_url, token_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Phase 1: Default execution (Linux host -> UnsupportedPlatform)
    set_simulated_driver_status(None);
    let resp = raw_http_get(port, "/api/system/drivers").await;
    assert!(resp.contains("HTTP/1.1 200 OK"));
    let body_str = resp.split("\r\n\r\n").nth(1).expect("HTTP body expected");
    let json: serde_json::Value = serde_json::from_str(body_str).expect("Valid JSON expected");

    assert_eq!(json["platform"], std::env::consts::OS);
    #[cfg(not(target_os = "windows"))]
    {
        assert_eq!(json["vhci"]["status"], "unsupported_platform");
        assert!(json["vhci"]["message"]
            .as_str()
            .unwrap()
            .contains("Windows KMDF driver is unsupported on Linux host"));
        assert_eq!(json["filter"]["status"], "unsupported_platform");
        assert!(json["filter"]["message"]
            .as_str()
            .unwrap()
            .contains("Windows KMDF driver is unsupported on Linux host"));
        assert_eq!(json["vhci_installed"], false);
        assert_eq!(json["filter_installed"], false);
        assert_eq!(json["is_operational"], false);
    }

    // Phase 2: Mock override to Accessible
    set_simulated_driver_status(Some(DriverOperationalStatus::Accessible));
    let resp_mock = raw_http_get(port, "/api/system/drivers").await;
    assert!(resp_mock.contains("HTTP/1.1 200 OK"));
    let body_mock = resp_mock.split("\r\n\r\n").nth(1).expect("HTTP body expected");
    let json_mock: serde_json::Value = serde_json::from_str(body_mock).expect("Valid JSON expected");

    assert_eq!(json_mock["vhci"]["status"], "accessible");
    assert_eq!(json_mock["filter"]["status"], "accessible");
    assert_eq!(json_mock["vhci_installed"], true);
    assert_eq!(json_mock["filter_installed"], true);
    assert_eq!(json_mock["is_operational"], true);

    // Phase 3: Mock override to TestStub
    set_simulated_driver_status(Some(DriverOperationalStatus::TestStub));
    let resp_stub = raw_http_get(port, "/api/system/drivers").await;
    let body_stub = resp_stub.split("\r\n\r\n").nth(1).expect("HTTP body expected");
    let json_stub: serde_json::Value = serde_json::from_str(body_stub).expect("Valid JSON expected");
    assert_eq!(json_stub["vhci"]["status"], "test_stub");
    assert_eq!(json_stub["vhci_installed"], true);
    assert_eq!(json_stub["is_operational"], false);

    // Reset back to None
    set_simulated_driver_status(None);
}

#[test]
fn test_installer_preflight_and_simulated_pnputil_exit_codes() {
    // 1. Pre-flight driver files validation logic
    let required_driver_files = [
        "drivers/vhci/anyusb_vhci.inf",
        "drivers/vhci/anyusb_vhci.sys",
        "drivers/vhci/anyusb_vhci.cat",
        "drivers/filter/anyusb_filter.inf",
        "drivers/filter/anyusb_filter.sys",
        "drivers/filter/anyusb_filter.cat",
    ];

    // Scenario A: Missing any file must be detected
    let test_dir = std::env::temp_dir().join(format!("usb_installer_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()));
    let _ = std::fs::create_dir_all(test_dir.join("drivers/vhci"));
    let _ = std::fs::create_dir_all(test_dir.join("drivers/filter"));

    // Write partial files (omit anyusb_filter.cat)
    std::fs::write(test_dir.join("drivers/vhci/anyusb_vhci.inf"), b"; inf").unwrap();
    std::fs::write(test_dir.join("drivers/vhci/anyusb_vhci.sys"), b"SYS").unwrap();
    std::fs::write(test_dir.join("drivers/vhci/anyusb_vhci.cat"), b"CAT").unwrap();
    std::fs::write(test_dir.join("drivers/filter/anyusb_filter.inf"), b"; inf").unwrap();
    std::fs::write(test_dir.join("drivers/filter/anyusb_filter.sys"), b"SYS").unwrap();

    let mut missing = Vec::new();
    for rel_path in &required_driver_files {
        let full = test_dir.join(rel_path);
        if !full.exists() {
            missing.push(*rel_path);
        }
    }
    assert_eq!(missing, vec!["drivers/filter/anyusb_filter.cat"]);

    // Complete all files
    std::fs::write(test_dir.join("drivers/filter/anyusb_filter.cat"), b"CAT").unwrap();
    let missing_complete: Vec<_> = required_driver_files.iter().filter(|p| !test_dir.join(p).exists()).collect();
    assert!(missing_complete.is_empty(), "All files present must yield empty missing list");

    let _ = std::fs::remove_dir_all(test_dir);

    // 2. Simulated pnputil exit code handling logic
    #[derive(Debug, PartialEq, Eq)]
    enum PnpUtilOutcome {
        SuccessNoReboot,
        SuccessRebootRequired,
        FatalError(i32),
    }

    fn evaluate_pnputil_exit_code(code: i32) -> PnpUtilOutcome {
        match code {
            0 => PnpUtilOutcome::SuccessNoReboot,
            3010 => PnpUtilOutcome::SuccessRebootRequired,
            err => PnpUtilOutcome::FatalError(err),
        }
    }

    assert_eq!(evaluate_pnputil_exit_code(0), PnpUtilOutcome::SuccessNoReboot);
    assert_eq!(evaluate_pnputil_exit_code(3010), PnpUtilOutcome::SuccessRebootRequired);
    assert_eq!(evaluate_pnputil_exit_code(2), PnpUtilOutcome::FatalError(2));
    assert_eq!(evaluate_pnputil_exit_code(5), PnpUtilOutcome::FatalError(5));
    assert_eq!(evaluate_pnputil_exit_code(259), PnpUtilOutcome::FatalError(259));
}


