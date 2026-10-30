//! Certified Indian e-Governance Device Adapters & Daemon Endpoints Test
//! Verifies:
//! 1. Identification and matching for all 15 certified Indian e-Gov devices across the 5 families.
//! 2. AdapterRegistry lookup and flag assignments (FLAG_RD_SERVICE_OPTIMIZED, FLAG_ISOCH_JITTER_BUFFER, FLAG_LOW_LATENCY_SERIAL).
//! 3. URB optimization pipelines, packet bounds checking, and target SLA specifications.
//! 4. Daemon endpoints: POST /api/license/activate, GET /api/license/status, GET /api/system/drivers.

use std::net::TcpListener;
use std::time::Duration;
use usb_master_core::adapters::*;
use usb_master_core::daemon::DaemonServer;
use usb_master_core::hardware_db::{
    DeviceCategory, FLAG_ISOCH_JITTER_BUFFER, FLAG_LOW_LATENCY_SERIAL, FLAG_RD_SERVICE_OPTIMIZED,
};
use usb_master_core::licensing::{
    LicenseClaims, LicenseEngine, STATIC_PRODUCTION_FIXTURE_HW_HASH, STATIC_PRODUCTION_FIXTURE_TOKEN,
};
use usb_master_core::protocol::{
    TransferType, TurboUrbHeader, CMD_SUBMIT_URB, FLAG_ANTICIPATORY, FLAG_COMPRESSED_ZSTD,
    FLAG_ISOCH_PACKET,
};

fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn reqwest_like_get(port: u16, path: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect to daemon test server");

    let req = format!("GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n", path, port);
    stream.write_all(req.as_bytes()).await.unwrap();

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf).to_string()
}

#[allow(dead_code)]
async fn reqwest_like_post(port: u16, path: &str, body: &serde_json::Value) -> String {
    let token = usb_master_core::daemon::read_secure_token_from_disk().unwrap_or_default();
    reqwest_like_post_with_token(port, path, body, &token).await
}

async fn reqwest_like_post_with_token(port: u16, path: &str, body: &serde_json::Value, token: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect to daemon test server");

    let payload = serde_json::to_string(body).unwrap();
    let req = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Daemon-Token: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path, port, token, payload.len(), payload
    );
    stream.write_all(req.as_bytes()).await.unwrap();

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf).to_string()
}

#[test]
fn test_identification_of_all_15_certified_indian_egov_devices() {
    let certified_devices: &[(u16, u16, &str, DeviceCategory, u32)] = &[
        // Family 1: Mantra MFS100 / MFS110 L1 (Fingerprint)
        (0x2137, 0x0020, "Mantra MFS100", DeviceCategory::FingerprintScanner, FLAG_RD_SERVICE_OPTIMIZED),
        (0x2137, 0x0041, "Mantra MFS110 L1", DeviceCategory::FingerprintScanner, FLAG_RD_SERVICE_OPTIMIZED),

        // Family 2: Idemia MorphoSmart MSO 1300 & Startek FM220U (Fingerprint)
        (0x083A, 0x0001, "Idemia MorphoSmart MSO 1300 E3", DeviceCategory::FingerprintScanner, FLAG_RD_SERVICE_OPTIMIZED),
        (0x083A, 0x0005, "Idemia MorphoSmart MSO 1300 L1", DeviceCategory::FingerprintScanner, FLAG_RD_SERVICE_OPTIMIZED),
        (0x0BCA, 0x8220, "Startek FM220U", DeviceCategory::FingerprintScanner, FLAG_RD_SERVICE_OPTIMIZED),

        // Family 3: 4:4:2 Ten-Print Slap Scanners
        (0x2137, 0x0060, "Mantra MFS600", DeviceCategory::SlapScanner, FLAG_RD_SERVICE_OPTIMIZED),
        (0x083A, 0x0050, "Idemia MorphoTop 100 / MORPHS", DeviceCategory::SlapScanner, FLAG_RD_SERVICE_OPTIMIZED),
        (0x1312, 0x2001, "Thales / 3M Cogent CS500e", DeviceCategory::SlapScanner, FLAG_RD_SERVICE_OPTIMIZED),
        (0x1312, 0x2002, "Thales / 3M Cogent CSD550", DeviceCategory::SlapScanner, FLAG_RD_SERVICE_OPTIMIZED),

        // Family 4: Dual Iris Scanners
        (0x2137, 0x0030, "Mantra MIS100V2 Dual Iris", DeviceCategory::IrisScanner, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER),
        (0x2137, 0x0031, "Mantra MATISX Dual Iris L1", DeviceCategory::IrisScanner, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER),
        (0x2914, 0x0100, "CMITECH BMT-20 / EMX-30", DeviceCategory::IrisScanner, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER),
        (0x1A22, 0x0010, "IriTech IriUniverse / MK2120U", DeviceCategory::IrisScanner, FLAG_RD_SERVICE_OPTIMIZED | FLAG_ISOCH_JITTER_BUFFER),

        // Family 5: GPS / GNSS Receivers
        (0x067B, 0x2303, "GlobalSat BU-353-S4", DeviceCategory::GpsDevice, FLAG_LOW_LATENCY_SERIAL),
        (0x10C4, 0xEA60, "G-Star IV GPS", DeviceCategory::GpsDevice, FLAG_LOW_LATENCY_SERIAL),
        (0x1546, 0x01A7, "BioEnable G2 GNSS", DeviceCategory::GpsDevice, FLAG_LOW_LATENCY_SERIAL),
    ];

    assert!(certified_devices.len() >= 15, "Must test at least 15 certified Indian e-Gov devices");

    for &(vid, pid, device_label, expected_category, expected_flags) in certified_devices {
        let adapter = AdapterRegistry::find_adapter(vid, pid)
            .unwrap_or_else(|| panic!("Failed to find certified adapter for {} (VID: 0x{:04X}, PID: 0x{:04X})", device_label, vid, pid));

        assert!(adapter.matches(vid, pid), "Adapter {} must match device", adapter.name());
        assert_eq!(adapter.category(), expected_category, "Category mismatch for {}", device_label);
        assert_eq!(adapter.required_flags(), expected_flags, "Flag mismatch for {}", device_label);
    }
}

#[test]
fn test_adapter_registry_registry_lookup_and_unknown_device_rejection() {
    let all = AdapterRegistry::all_adapters();
    assert_eq!(all.len(), 5, "Certified registry must register all 5 hardware families");

    // Unknown rogue VID/PID must return None
    assert!(AdapterRegistry::find_adapter(0xDEAD, 0xBEEF).is_none());
    assert!(AdapterRegistry::find_adapter(0x0000, 0x0000).is_none());
}

#[test]
fn test_sla_targets_and_latency_thresholds() {
    // 1. Mantra MFS100: SLA target < 120ms
    let mantra = MantraMfs100Adapter::new();
    assert!(mantra.target_sla_ms() <= 120, "Mantra SLA must be <= 120ms");

    // 2. Morpho MSO 1300: SLA target < 120ms
    let morpho = MorphoMso1300Adapter::new();
    assert!(morpho.target_sla_ms() <= 120, "Morpho SLA must be <= 120ms");

    // 3. Slap Scanners: SLA target < 250ms
    let slap = SlapScannersAdapter::new();
    assert!(slap.target_sla_ms() <= 250, "Slap scanner SLA must be <= 250ms");

    // 4. Iris Scanners: SLA target < 180ms
    let iris = IrisScannersAdapter::new();
    assert!(iris.target_sla_ms() <= 180, "Iris scanner SLA must be <= 180ms");

    // 5. GPS / GNSS: Low latency serial target <= 50ms
    let gps = GpsGnssAdapter::new();
    assert!(gps.target_sla_ms() <= 50, "GPS serial SLA must be <= 50ms");
}

#[test]
fn test_urb_optimization_pipelines() {
    // 1. Mantra MFS100: Bulk URB should receive FLAG_ANTICIPATORY and biometric compression
    let mantra = MantraMfs100Adapter::new();
    let header = TurboUrbHeader::new(CMD_SUBMIT_URB, 1, 1, 0x81, TransferType::Bulk, 512);
    // Uncompressed biometric fingerprint raster pattern (highly repetitive)
    let raw_fp_data = vec![0x12; 1024];
    let mut mantra_msg = UrbWireMessage::new(header, raw_fp_data);
    mantra.optimize_urb(&mut mantra_msg).expect("Optimization must succeed");
    assert!((mantra_msg.header.flags & FLAG_ANTICIPATORY) != 0, "Bulk transfer must have anticipatory flag");
    assert!((mantra_msg.header.flags & FLAG_COMPRESSED_ZSTD) != 0, "Biometric frame must be compressed");
    assert!(mantra_msg.payload.len() < 1024, "Payload must be smaller after compression");

    // 2. Morpho MSO 1300: Bulk transfer acceleration
    let morpho = MorphoMso1300Adapter::new();
    let mut morpho_msg = UrbWireMessage::new(
        TurboUrbHeader::new(CMD_SUBMIT_URB, 2, 1, 0x82, TransferType::Bulk, 256),
        vec![0xFF; 256],
    );
    morpho.optimize_urb(&mut morpho_msg).unwrap();
    assert!((morpho_msg.header.flags & FLAG_ANTICIPATORY) != 0);

    // 3. Slap Scanners: High-throughput Zstd-1 compression on large 4:4:2 slap frames
    let slap = SlapScannersAdapter::new();
    let large_slap_frame = vec![0x80; 65536]; // 64 KB slap frame
    let mut slap_msg = UrbWireMessage::new(
        TurboUrbHeader::new(CMD_SUBMIT_URB, 3, 1, 0x81, TransferType::Bulk, 65536),
        large_slap_frame,
    );
    slap.optimize_urb(&mut slap_msg).unwrap();
    assert!((slap_msg.header.flags & FLAG_COMPRESSED_ZSTD) != 0);
    assert!(slap_msg.payload.len() < 1000, "Zstd-1 should compress repeating frame to tiny size");

    // 4. Iris Scanners: Isochronous jitter buffer framing
    let iris = IrisScannersAdapter::new();
    let mut iris_msg = UrbWireMessage::new(
        TurboUrbHeader::new(CMD_SUBMIT_URB, 4, 1, 0x83, TransferType::Isochronous, 1024),
        vec![0x44; 1024],
    );
    iris.optimize_urb(&mut iris_msg).unwrap();
    assert!((iris_msg.header.flags & FLAG_ISOCH_PACKET) != 0, "Isochronous packet flag required");
    assert!((iris_msg.header.flags & FLAG_ANTICIPATORY) != 0, "Anticipatory flag required");

    // 5. GPS / GNSS: 1 Hz NMEA sentence preservation
    let gps = GpsGnssAdapter::new();
    let nmea_sentence = b"$GPRMC,123519,A,2836.871,N,07712.339,E,022.4,084.4,230326,003.1,W*6A\r\n";
    let mut gps_msg = UrbWireMessage::new(
        TurboUrbHeader::new(CMD_SUBMIT_URB, 5, 1, 0x81, TransferType::Bulk, nmea_sentence.len() as u32),
        nmea_sentence.to_vec(),
    );
    gps.optimize_urb(&mut gps_msg).unwrap();
    assert!((gps_msg.header.flags & FLAG_ANTICIPATORY) != 0);

    // 6. Packet size limits: GPS maximum size is 512 bytes
    let mut oversize_gps = UrbWireMessage::new(
        TurboUrbHeader::new(CMD_SUBMIT_URB, 6, 1, 0x81, TransferType::Bulk, 1024),
        vec![0xAA; 1024],
    );
    let err = gps.optimize_urb(&mut oversize_gps).unwrap_err();
    match err {
        AdapterError::PacketTooLarge { max_size, actual_size } => {
            assert_eq!(max_size, 512);
            assert_eq!(actual_size, 1024);
        }
        _ => panic!("Expected PacketTooLarge error"),
    }
}

#[tokio::test]
async fn test_daemon_license_and_driver_endpoints() {
    // Set test hardware hash to match static production fixture before launching daemon
    std::env::set_var("BHARTX_TEST_HARDWARE_HASH", STATIC_PRODUCTION_FIXTURE_HW_HASH);

    let port = find_free_port();
    let test_token = format!("test_adapter_token_{}", port);
    let token_clone = test_token.clone();
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run_with_token(port, broker_url, token_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    // 1. GET /api/system/drivers
    let drivers_resp = reqwest_like_get(port, "/api/system/drivers").await;
    assert!(drivers_resp.contains("HTTP/1.1 200 OK"));
    #[cfg(target_os = "windows")]
    assert!(drivers_resp.contains("\"vhci_installed\":"));
    #[cfg(not(target_os = "windows"))]
    assert!(drivers_resp.contains("\"vhci_installed\":false"));
    assert!(drivers_resp.contains("\"udp443_reachable\":true"));
    assert!(drivers_resp.contains("\"kernel_version\":\"1.33.0\""));

    // 2. GET /api/identity to get local hardware_hash
    let identity_resp = reqwest_like_get(port, "/api/identity").await;
    let identity_json: serde_json::Value = serde_json::from_str(identity_resp.split("\r\n\r\n").nth(1).unwrap())
        .expect("Must parse identity JSON");
    let hardware_hash = identity_json["hardware_hash"].as_str().unwrap();
    let formatted_id = identity_json["formatted_id"].as_str().unwrap();
    assert_eq!(hardware_hash, STATIC_PRODUCTION_FIXTURE_HW_HASH);

    // 3. GET /api/license/status before activation
    let status_resp_unlicensed = reqwest_like_get(port, "/api/license/status").await;
    assert!(status_resp_unlicensed.contains("HTTP/1.1 200 OK"));
    assert!(status_resp_unlicensed.contains("hardware_hash"));
    assert!(status_resp_unlicensed.contains("formatted_id"));

    // 4. POST /api/license/activate with an invalid token -> returns 400 Bad Request
    let bad_payload = serde_json::json!({
        "token": "invalid.license.token"
    });
    let bad_activate_resp = reqwest_like_post_with_token(port, "/api/license/activate", &bad_payload, &test_token).await;
    assert!(bad_activate_resp.contains("HTTP/1.1 400 Bad Request"));
    assert!(bad_activate_resp.contains("\"success\":false"));

    // 5. POST /api/license/activate with valid static production fixture token -> returns 200 OK with claims
    let good_payload = serde_json::json!({
        "token": STATIC_PRODUCTION_FIXTURE_TOKEN
    });
    let good_activate_resp = reqwest_like_post_with_token(port, "/api/license/activate", &good_payload, &test_token).await;
    assert!(good_activate_resp.contains("HTTP/1.1 200 OK"));
    assert!(good_activate_resp.contains("\"success\":true"));
    assert!(good_activate_resp.contains("UIDAI Aadhaar Seva Kendra"));
    assert!(good_activate_resp.contains("EnterprisePro"));

    // 6. GET /api/license/status after activation -> returns active status with claims
    let status_resp_active = reqwest_like_get(port, "/api/license/status").await;
    assert!(status_resp_active.contains("HTTP/1.1 200 OK"));
    let status_json: serde_json::Value = serde_json::from_str(status_resp_active.split("\r\n\r\n").nth(1).unwrap())
        .expect("Must parse status JSON");

    assert_eq!(status_json["status"], "active");
    assert_eq!(status_json["active"], true);
    assert_eq!(status_json["tier"], "EnterprisePro");
    assert_eq!(status_json["customer_name"], "UIDAI Aadhaar Seva Kendra");
    assert_eq!(status_json["hardware_hash"], STATIC_PRODUCTION_FIXTURE_HW_HASH);
    assert_eq!(status_json["formatted_id"], formatted_id);
    assert_eq!(status_json["license_id"], "LIC-2026-BHARTX-CERTIFIED");

    // Clean up env override
    std::env::remove_var("BHARTX_TEST_HARDWARE_HASH");
}
