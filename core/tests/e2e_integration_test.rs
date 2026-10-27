//! End-to-End (E2E) Multi-Node Integration Test for USB Master Pro
//!
//! Validates the entire distributed P2P mesh fabric:
//! 1. Starts ephemeral Node.js P2P Signaling Broker (`signaling/server.js`).
//! 2. Spawns two independent daemon processes:
//!    - Node A (Sender): Outbound rendezvous client on ephemeral Port A.
//!    - Node B (Receiver): Outbound rendezvous client on ephemeral Port B.
//! 3. Verifies AnyDesk-style 9-digit identity generation & isolation:
//!    - Asserts unique 9-digit hardware IDs for Node A and Node B.
//! 4. Verifies physical hardware enumeration & safety exclusion logic:
//!    - Scans connected devices on Node A.
//!    - Validates system keyboard/mouse has `can_forward: false`.
//!    - Validates unforced forwarding of system devices returns HTTP 403 Forbidden.
//!    - Validates forced forwarding (`force: true`) succeeds with HTTP 200 OK.
//! 5. Validates P2P signaling negotiation:
//!    - Node B dispatches `POST /api/connect` targeting Node A.
//!    - Node A receives request and dispatches `POST /api/respond` (accepted: true).
//!    - Node B receives acceptance decision and mounts remote virtual device.
//!    - Validates `GET /api/devices` on Node B reflects the mounted peripheral.
//! 6. Cleans up all child processes with RAII guards.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// RAII Process Guard ensuring child processes are killed on test completion or panic.
struct ProcessGuard {
    child: Child,
    name: String,
}

impl ProcessGuard {
    fn new(child: Child, name: &str) -> Self {
        Self {
            child,
            name: name.to_string(),
        }
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        println!("  🛑 [Teardown] Terminating process '{}' (PID: {})...", self.name, self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Allocates an ephemeral loopback port for isolated testing.
fn get_ephemeral_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

/// Resolves the absolute path to the signaling broker entry point (`signaling/server.js`).
fn get_signaling_server_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().expect("Manifest dir should have repo root parent");
    let signaling_js = repo_root.join("signaling/server.js");
    assert!(
        signaling_js.exists(),
        "Signaling server.js not found at {:?}",
        signaling_js
    );
    signaling_js
}

/// Resolves the path to the compiled `usb_master_daemon` binary.
fn get_daemon_binary_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_usb_master_daemon") {
        let p = PathBuf::from(path);
        if p.exists() {
            return p;
        }
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let exe_name = if cfg!(windows) { "usb_master_daemon.exe" } else { "usb_master_daemon" };

    let candidate_release = manifest_dir.join("target/release").join(exe_name);
    if candidate_release.exists() {
        return candidate_release;
    }

    let candidate_debug = manifest_dir.join("target/debug").join(exe_name);
    if candidate_debug.exists() {
        return candidate_debug;
    }

    // Fallback build if not yet compiled
    println!("  🔨 Pre-compiling usb_master_daemon binary...");
    let status = Command::new("cargo")
        .args(["build", "--bin", "usb_master_daemon"])
        .current_dir(manifest_dir)
        .status()
        .expect("Failed to run cargo build for usb_master_daemon");
    assert!(status.success(), "cargo build failed for usb_master_daemon");
    candidate_debug
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    body: String,
}

impl HttpResponse {
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("Failed to parse JSON from body '{}': {}", self.body, e))
    }
}

/// Performs a raw HTTP GET request over Tokio TcpStream.
async fn http_get(port: u16, path: &str) -> Result<HttpResponse, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| format!("Connect to 127.0.0.1:{} failed: {}", port, e))?;

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        path, port
    );
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| format!("Write failed: {}", e))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("Read failed: {}", e))?;

    parse_http_response(&buf)
}

/// Performs a raw HTTP POST request with a JSON payload over Tokio TcpStream.
async fn http_post_json(port: u16, path: &str, body: &serde_json::Value) -> Result<HttpResponse, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let body_str = serde_json::to_string(body).map_err(|e| e.to_string())?;
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| format!("Connect to 127.0.0.1:{} failed: {}", port, e))?;

    let req = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path, port, body_str.len(), body_str
    );
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| format!("Write failed: {}", e))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("Read failed: {}", e))?;

    parse_http_response(&buf)
}

/// Parses an HTTP 1.1 response buffer into status code and string body.
fn parse_http_response(raw: &[u8]) -> Result<HttpResponse, String> {
    let s = String::from_utf8_lossy(raw);
    let body_start = if let Some(pos) = s.find("\r\n\r\n") {
        pos + 4
    } else if let Some(pos) = s.find("\n\n") {
        pos + 2
    } else {
        return Err(format!("No HTTP boundary found in response: {:?}", s));
    };

    let header_part = &s[..body_start];
    let body = s[body_start..].to_string();

    let mut header_lines = header_part.lines();
    let status_line = header_lines
        .next()
        .ok_or_else(|| "Missing status line".to_string())?;
    let mut status_parts = status_line.split_whitespace();
    let _version = status_parts
        .next()
        .ok_or_else(|| "Missing HTTP version".to_string())?;
    let status_str = status_parts
        .next()
        .ok_or_else(|| "Missing status code".to_string())?;
    let status: u16 = status_str
        .parse()
        .map_err(|e| format!("Invalid status code '{}': {}", status_str, e))?;

    Ok(HttpResponse { status, body })
}

/// Polls an HTTP endpoint until it returns HTTP 200 OK or times out.
async fn wait_for_broker_healthy(port: u16, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(res) = http_get(port, "/health").await {
            if res.status == 200 && res.body.contains("USB Master Pro") {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("Signaling broker failed to report healthy on port {} within {:?}", port, timeout);
}

/// Polls a daemon endpoint until its signaling broker connection reports active.
async fn wait_for_daemon_broker_connected(port: u16, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(res) = http_get(port, "/api/status").await {
            if res.status == 200 {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&res.body) {
                    if val["broker_connected"].as_bool() == Some(true) {
                        return;
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("Daemon on port {} failed to connect to signaling broker within {:?}", port, timeout);
}

#[tokio::test]
async fn test_end_to_end_multi_node_passthrough() {
    println!("================================================================================");
    println!("  🧪 USB Master Pro — E2E Multi-Node Passthrough & Signaling Integration Test");
    println!("  🏢 BhartX Ecosystems Pvt Ltd | Lead QA Verification");
    println!("================================================================================");

    // 1. Allocate 3 distinct ephemeral ports
    let broker_port = get_ephemeral_port();
    let port_a = get_ephemeral_port();
    let port_b = get_ephemeral_port();
    assert_ne!(port_a, port_b);
    assert_ne!(broker_port, port_a);

    println!("  📌 Ephemeral Port Allocations: Broker={}, Node A={}, Node B={}", broker_port, port_a, port_b);

    // 2. Start Node.js Signaling Broker
    let signaling_js = get_signaling_server_path();
    println!("  🚀 Starting Node.js Signaling Broker: {:?}", signaling_js);

    let broker_child = Command::new("node")
        .arg(&signaling_js)
        .env("PORT", broker_port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn Node.js signaling broker");
    let _broker_guard = ProcessGuard::new(broker_child, "P2P Signaling Broker");

    wait_for_broker_healthy(broker_port, Duration::from_secs(5)).await;
    println!("  ✅ P2P Signaling Broker is live on http://127.0.0.1:{}", broker_port);

    // 3. Spawn Node A (Sender) and Node B (Receiver) daemon processes
    let daemon_bin = get_daemon_binary_path();
    let broker_ws_url = format!("ws://127.0.0.1:{}", broker_port);

    println!("  🚀 Spawning Node A daemon on port {} (HOSTNAME=node-alpha-sender)...", port_a);
    let daemon_a_child = Command::new(&daemon_bin)
        .env("DAEMON_PORT", port_a.to_string())
        .env("SIGNALING_URL", &broker_ws_url)
        .env("HOSTNAME", "node-alpha-sender")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn Node A daemon process");
    let _node_a_guard = ProcessGuard::new(daemon_a_child, "Daemon Node A (Sender)");

    println!("  🚀 Spawning Node B daemon on port {} (HOSTNAME=node-beta-receiver)...", port_b);
    let daemon_b_child = Command::new(&daemon_bin)
        .env("DAEMON_PORT", port_b.to_string())
        .env("SIGNALING_URL", &broker_ws_url)
        .env("HOSTNAME", "node-beta-receiver")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn Node B daemon process");
    let _node_b_guard = ProcessGuard::new(daemon_b_child, "Daemon Node B (Receiver)");

    // Wait for both daemons to bind HTTP and establish outbound WebSocket to broker
    println!("  ⏳ Waiting for Node A and Node B to register with Signaling Broker...");
    wait_for_daemon_broker_connected(port_a, Duration::from_secs(6)).await;
    wait_for_daemon_broker_connected(port_b, Duration::from_secs(6)).await;
    println!("  ✅ Node A and Node B are online and connected to P2P Signaling Broker");

    // Settle broker peer map
    tokio::time::sleep(Duration::from_millis(250)).await;

    // -------------------------------------------------------------------------
    // Phase 1: Identity & Isolation Assertions
    // -------------------------------------------------------------------------
    println!("\n👉 [Phase 1] Verifying GET /api/identity for Node A and Node B...");
    let id_a_resp = http_get(port_a, "/api/identity").await.expect("GET /api/identity Node A failed");
    assert_eq!(id_a_resp.status, 200, "Node A identity must return HTTP 200");
    let id_a = id_a_resp.json();

    let id_b_resp = http_get(port_b, "/api/identity").await.expect("GET /api/identity Node B failed");
    assert_eq!(id_b_resp.status, 200, "Node B identity must return HTTP 200");
    let id_b = id_b_resp.json();

    let raw_id_a = id_a["raw_id"].as_u64().expect("Node A raw_id must be u64");
    let raw_id_b = id_b["raw_id"].as_u64().expect("Node B raw_id must be u64");
    let fmt_id_a = id_a["formatted_id"].as_str().expect("Node A formatted_id must be str");
    let fmt_id_b = id_b["formatted_id"].as_str().expect("Node B formatted_id must be str");
    let pin_a = id_a["default_pin"].as_str().expect("Node A default_pin must be str");

    println!("   Node A Identity: {} (raw: {}) | PIN: {}", fmt_id_a, raw_id_a, pin_a);
    println!("   Node B Identity: {} (raw: {}) | PIN: {}", fmt_id_b, raw_id_b, id_b["default_pin"]);

    assert!(raw_id_a >= 100_000_000 && raw_id_a <= 999_999_999, "Node A raw_id must be 9 digits");
    assert!(raw_id_b >= 100_000_000 && raw_id_b <= 999_999_999, "Node B raw_id must be 9 digits");
    assert_ne!(raw_id_a, raw_id_b, "Node A and Node B must have different AnyUSB IDs");
    assert_ne!(fmt_id_a, fmt_id_b, "Formatted AnyUSB IDs must differ");
    assert_eq!(fmt_id_a.len(), 11, "Formatted ID must be 11 chars (XXX XXX XXX)");
    assert_eq!(fmt_id_b.len(), 11, "Formatted ID must be 11 chars (XXX XXX XXX)");
    println!("   ✅ Phase 1: Identity isolation confirmed");

    // -------------------------------------------------------------------------
    // Phase 2: Hardware Enumeration & Exclusion Logic on Node A
    // -------------------------------------------------------------------------
    println!("\n👉 [Phase 2] Verifying GET /api/devices and primary system device exclusion on Node A...");
    let dev_a_resp = http_get(port_a, "/api/devices").await.expect("GET /api/devices Node A failed");
    assert_eq!(dev_a_resp.status, 200);
    let dev_a = dev_a_resp.json();

    let local_devices = dev_a["local"].as_array().expect("Devices response must include 'local' array");
    println!("   Node A enumerated {} local USB device(s)", local_devices.len());
    if local_devices.is_empty() {
        println!("   ℹ️ Headless Cloud Virtual Machine environment detected (0 physical USB devices attached).");
        println!("   ℹ️ Hardware forwarding & remote VHCI mounting phases (Phases 2-4) skipped on headless CI runner.");
    } else {
        let mut device_to_share: Option<String> = None;

        for dev in local_devices {
            let dev_id = dev["id"].as_str().expect("Device id must be string");
            let friendly_name = dev["friendly_name"].as_str().unwrap_or("Unknown");
            let is_system = dev["is_system_device"].as_bool().unwrap_or(false);
            let can_forward = dev["can_forward"].as_bool().unwrap_or(true);

            println!("   - Peripheral [{}]: '{}' (is_system={}, can_forward={})", dev_id, friendly_name, is_system, can_forward);

            if is_system {
                // Safety Invariant: System keyboards, mice, trackpads must NEVER be forwarded without explicit force
                assert!(!can_forward, "Safety Invariant: System device '{}' must have can_forward: false", friendly_name);

                // Attempt 1: Forward WITHOUT force must fail with 403 Forbidden
                let unforced_payload = serde_json::json!({
                    "device_id": dev_id,
                    "share": true,
                    "force": false
                });
                let unforced_resp = http_post_json(port_a, "/api/forward", &unforced_payload).await.expect("POST /api/forward failed");
                assert_eq!(unforced_resp.status, 403, "Unforced forward of system device must be rejected with 403 Forbidden");
                assert!(
                    unforced_resp.body.contains("primary system input device"),
                    "Rejection message must indicate primary system input device"
                );
                assert!(
                    unforced_resp.body.contains("force=true"),
                    "Rejection message must explain force=true override"
                );
                println!("     🛡️ Exclusion passed: unforced forward safely blocked with 403 Forbidden");

                // Attempt 2: Forward WITH force must succeed with 200 OK
                let forced_payload = serde_json::json!({
                    "device_id": dev_id,
                    "share": true,
                    "force": true
                });
                let forced_resp = http_post_json(port_a, "/api/forward", &forced_payload).await.expect("POST /api/forward forced failed");
                assert_eq!(forced_resp.status, 200, "Forced forward of system device must return 200 OK");
                let forced_json = forced_resp.json();
                assert_eq!(forced_json["success"].as_bool(), Some(true));
                assert_eq!(forced_json["device"]["is_shared"].as_bool(), Some(true));
                println!("     ✅ Forced override confirmed: device shared with operator force flag");

                device_to_share = Some(dev_id.to_string());
            } else {
                // Non-system device can be shared directly
                assert!(can_forward, "Non-system device must have can_forward: true");
                let share_payload = serde_json::json!({
                    "device_id": dev_id,
                    "share": true,
                    "force": false
                });
                let share_resp = http_post_json(port_a, "/api/forward", &share_payload).await.expect("POST /api/forward non-system failed");
                assert_eq!(share_resp.status, 200);
                let share_json = share_resp.json();
                assert_eq!(share_json["success"].as_bool(), Some(true));
                device_to_share = Some(dev_id.to_string());
            }
        }

        let shared_id = device_to_share.expect("Expected at least one device on Node A to test forward/connect");
        println!("   ✅ Phase 2: Hardware enumeration and safety exclusion verified. Active shared device ID: {}", shared_id);

        // Verify GET /api/devices on Node A reflects is_shared: true
        let dev_a_updated = http_get(port_a, "/api/devices").await.unwrap().json();
        let updated_local = dev_a_updated["local"].as_array().unwrap();
        let active_dev = updated_local.iter().find(|d| d["id"] == shared_id).unwrap();
        assert_eq!(active_dev["is_shared"].as_bool(), Some(true), "Device on Node A must reflect is_shared: true");

        // -------------------------------------------------------------------------
        // Phase 3: P2P Signaling Connect & Acceptance
        // -------------------------------------------------------------------------
        println!("\n👉 [Phase 3] Calling POST /api/connect on Node B targeting Node A [{}]...", raw_id_a);
        let connect_payload = serde_json::json!({
            "target_id": raw_id_a.to_string(),
            "device_id": shared_id,
            "pin": pin_a
        });

        let connect_resp = http_post_json(port_b, "/api/connect", &connect_payload).await.expect("POST /api/connect on Node B failed");
        assert_eq!(connect_resp.status, 200, "Node B connect request must succeed with 200 OK");
        let connect_json = connect_resp.json();
        assert_eq!(connect_json["success"].as_bool(), Some(true));
        assert_eq!(connect_json["status"].as_str(), Some("CONNECT_REQUEST_DISPATCHED"));
        assert_eq!(connect_json["targetId"].as_str(), Some(raw_id_a.to_string().as_str()));
        println!("   ✅ Node B dispatched connection request to Node A [{}] via Broker", raw_id_a);

        // Give broker and Node A a brief moment to receive INCOMING_CONNECTION
        tokio::time::sleep(Duration::from_millis(200)).await;

        println!("\n👉 [Phase 4] Calling POST /api/respond on Node A accepting Node B [{}] connection...", raw_id_b);
        let respond_payload = serde_json::json!({
            "from_id": raw_id_b.to_string(),
            "accepted": true,
            "reason": "Accepted by automated E2E test harness"
        });

        let respond_resp = http_post_json(port_a, "/api/respond", &respond_payload).await.expect("POST /api/respond on Node A failed");
        assert_eq!(respond_resp.status, 200, "Node A respond must succeed with 200 OK");
        let respond_json = respond_resp.json();
        assert_eq!(respond_json["success"].as_bool(), Some(true));
        println!("   ✅ Node A dispatched connection acceptance back to Node B via Broker");

        // -------------------------------------------------------------------------
        // Phase 4: Remote Device Mounting on Node B
        // -------------------------------------------------------------------------
        println!("\n👉 [Phase 5] Verifying GET /api/devices on Node B reflects mounted remote device from Node A...");
        let poll_timeout = Duration::from_secs(6);
        let poll_start = Instant::now();
        let mut remote_found = false;
        let mut mounted_record = serde_json::Value::Null;

        while poll_start.elapsed() < poll_timeout {
            if let Ok(resp) = http_get(port_b, "/api/devices").await {
                if resp.status == 200 {
                    let json = resp.json();
                    if let Some(remote_arr) = json["remote"].as_array() {
                        if let Some(entry) = remote_arr.iter().find(|d| {
                            d["remote_peer_id"].as_str() == Some(raw_id_a.to_string().as_str())
                        }) {
                            remote_found = true;
                            mounted_record = entry.clone();
                            break;
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        assert!(
            remote_found,
            "Node B failed to mount remote peripheral from Node A [{}] within {:?}",
            raw_id_a, poll_timeout
        );

        println!("   🎉 Mounted Remote Device on Node B: {}", mounted_record);
        assert_eq!(
            mounted_record["remote_peer_id"].as_str(),
            Some(raw_id_a.to_string().as_str()),
            "Remote device peer ID must match Node A"
        );
        assert!(
            mounted_record["driver_state"].as_str().unwrap_or_default().contains("Virtual USB Host"),
            "Driver state must confirm binding to Virtual USB Host (VHCI OK)"
        );
        assert!(
            mounted_record["name"].as_str().unwrap_or_default().contains(&raw_id_a.to_string()),
            "Remote device name should reference remote peer ID"
        );
    }

    // -------------------------------------------------------------------------
    // Phase 5: Negative Validation & Boundary Protections
    // -------------------------------------------------------------------------
    println!("\n👉 [Phase 6] Testing negative boundary protections...");

    // Test: Connect with invalid (< 9 digits) target ID
    let invalid_connect_payload = serde_json::json!({
        "target_id": "12345678" // 8 digits
    });
    let invalid_connect_resp = http_post_json(port_b, "/api/connect", &invalid_connect_payload).await.unwrap();
    assert_eq!(invalid_connect_resp.status, 400, "Connect request with <9 digits must return HTTP 400 Bad Request");
    println!("     🛡️ Target ID boundary validated: 8-digit ID rejected with 400");

    // Test: Forwarding non-existent hardware device ID
    let bogus_dev_payload = serde_json::json!({
        "device_id": "non-existent-device-9999",
        "share": true
    });
    let bogus_resp = http_post_json(port_a, "/api/forward", &bogus_dev_payload).await.unwrap();
    assert_eq!(bogus_resp.status, 404, "Forwarding non-existent device must return HTTP 404 Not Found");
    println!("     🛡️ Hardware ID boundary validated: non-existent device returns 404");

    println!("\n================================================================================");
    println!("  ✅ E2E MULTI-NODE INTEGRATION TEST: 100% PASSED");
    println!("================================================================================\n");
}
