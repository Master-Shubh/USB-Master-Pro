//! Daemon IPC Integration Tests
//! Verifies HTTP REST endpoints and WebSocket event streams.

use std::net::TcpListener;
use std::time::Duration;
use usb_master_core::daemon::DaemonServer;

fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

#[tokio::test]
async fn test_daemon_endpoints_lifecycle() {
    let port = find_free_port();
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run(port, broker_url).await;
    });

    // Wait for server to bind
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 1. Test GET /api/identity
    let client = reqwest_like_get(port, "/api/identity").await;
    assert!(client.contains("formatted_id"), "Identity endpoint must contain formatted_id");
    assert!(client.contains("raw_id"), "Identity endpoint must contain raw_id");

    // 2. Test GET /api/devices
    let devices_resp = reqwest_like_get(port, "/api/devices").await;
    assert!(devices_resp.contains("local"), "Devices response must contain 'local' array");

    // 3. Test GET /api/status
    let status_resp = reqwest_like_get(port, "/api/status").await;
    assert!(status_resp.contains("active"), "Status endpoint must report active");
    assert!(status_resp.contains("turbo_urb_throughput_mb_s"), "Telemetry must report throughput");

    // 4. Test Embedded WebUI root route "/"
    let index_resp = reqwest_like_get(port, "/").await;
    assert!(index_resp.contains("HTTP/1.1 200 OK"), "Root should return 200 OK");
    assert!(index_resp.contains("text/html"), "Root should return text/html");
    assert!(index_resp.contains("USB Master Pro"), "Root should contain USB Master Pro");

    // 5. Test Embedded WebUI aliases "/ui" and "/index.html"
    let ui_resp = reqwest_like_get(port, "/ui").await;
    assert!(ui_resp.contains("HTTP/1.1 200 OK"), "/ui should return 200 OK");
    assert!(ui_resp.contains("text/html"), "/ui should return text/html");

    let index_html_resp = reqwest_like_get(port, "/index.html").await;
    assert!(index_html_resp.contains("HTTP/1.1 200 OK"), "/index.html should return 200 OK");

    // 6. Test Embedded Static Assets: logo, favicon, icons
    let logo_resp = reqwest_like_get(port, "/assets/logo.png").await;
    assert!(logo_resp.contains("HTTP/1.1 200 OK"), "Logo should return 200 OK");
    assert!(logo_resp.contains("image/png"), "Logo should return image/png");

    let favicon_resp = reqwest_like_get(port, "/assets/favicon.ico").await;
    assert!(favicon_resp.contains("HTTP/1.1 200 OK"), "Favicon should return 200 OK");
    assert!(favicon_resp.contains("image/x-icon"), "Favicon should return image/x-icon");

    let icon64_resp = reqwest_like_get(port, "/assets/icon_64.png").await;
    assert!(icon64_resp.contains("HTTP/1.1 200 OK"), "Icon 64 should return 200 OK");
    assert!(icon64_resp.contains("image/png"), "Icon 64 should return image/png");

    let icon256_resp = reqwest_like_get(port, "/assets/icon_256.png").await;
    assert!(icon256_resp.contains("HTTP/1.1 200 OK"), "Icon 256 should return 200 OK");
    assert!(icon256_resp.contains("image/png"), "Icon 256 should return image/png");
}

#[tokio::test]
async fn test_daemon_active_session_fsm_lifecycle() {
    let port = find_free_port();
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run(port, broker_url).await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    // 1. Initiate connection: POST /api/connect -> transitions to Pairing
    let connect_payload = serde_json::json!({
        "target_id": "123456789",
        "device_id": "ep-test-dev"
    });
    let connect_resp = reqwest_like_post(port, "/api/connect", &connect_payload).await;
    assert!(connect_resp.contains("HTTP/1.1 200 OK"));
    assert!(connect_resp.contains("CONNECT_REQUEST_DISPATCHED"));
    assert!(connect_resp.contains("Pairing"));

    // 2. GET /api/devices should now include real active session in Pairing state
    let devices_resp = reqwest_like_get(port, "/api/devices").await;
    assert!(devices_resp.contains("sessions"));
    assert!(devices_resp.contains("123456789"));
    assert!(devices_resp.contains("Pairing"));

    // 3. GET /api/sessions should directly return the active session map
    let sessions_resp = reqwest_like_get(port, "/api/sessions").await;
    assert!(sessions_resp.contains("123456789"));
    assert!(sessions_resp.contains("Pairing"));

    // 4. Operator accepts connection: POST /api/respond (accepted = true)
    // Transitions Pairing -> Negotiating -> Connecting -> PreparingDevice -> Active
    let accept_payload = serde_json::json!({
        "from_id": "123456789",
        "accepted": true,
        "reason": "Operator approved session"
    });
    let accept_resp = reqwest_like_post(port, "/api/respond", &accept_payload).await;
    assert!(accept_resp.contains("HTTP/1.1 200 OK"));

    // 5. GET /api/devices confirms state transitioned to Active
    let devices_active = reqwest_like_get(port, "/api/devices").await;
    assert!(devices_active.contains("Active"));
    assert!(devices_active.contains("metrics"));
    assert!(devices_active.contains("throughput_mb_s"));

    // 6. Operator disconnects: POST /api/disconnect -> transitions Closing -> Ready
    let disconnect_payload = serde_json::json!({
        "peer_id": "123456789",
        "reason": "User completed task"
    });
    let disconnect_resp = reqwest_like_post(port, "/api/disconnect", &disconnect_payload).await;
    assert!(disconnect_resp.contains("HTTP/1.1 200 OK"));
    assert!(disconnect_resp.contains("DISCONNECTED"));

    // 7. GET /api/devices confirms session is now Ready
    let devices_ready = reqwest_like_get(port, "/api/devices").await;
    assert!(devices_ready.contains("Ready"));
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

async fn reqwest_like_post(port: u16, path: &str, body: &serde_json::Value) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect to daemon test server");

    let payload = serde_json::to_string(body).unwrap();
    let req = format!(
        "POST {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path, port, payload.len(), payload
    );
    stream.write_all(req.as_bytes()).await.unwrap();

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf).to_string()
}
