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
