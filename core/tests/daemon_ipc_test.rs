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
    let test_token = format!("test_endpoints_token_{}", port);
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run_with_token(port, broker_url, test_token).await;
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
    let test_token = format!("test_session_token_{}", port);
    let token_clone = test_token.clone();
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run_with_token(port, broker_url, token_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;

    // 1. Initiate connection: POST /api/connect -> transitions to Pairing
    let connect_payload = serde_json::json!({
        "target_id": "123456789",
        "device_id": "ep-test-dev"
    });
    let connect_resp = reqwest_like_post_with_token(port, "/api/connect", &connect_payload, &test_token).await;
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
    let accept_resp = reqwest_like_post_with_token(port, "/api/respond", &accept_payload, &test_token).await;
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
    let disconnect_resp = reqwest_like_post_with_token(port, "/api/disconnect", &disconnect_payload, &test_token).await;
    assert!(disconnect_resp.contains("HTTP/1.1 200 OK"));
    assert!(disconnect_resp.contains("DISCONNECTED"));

    // 7. GET /api/devices confirms session is now Ready
    let devices_ready = reqwest_like_get(port, "/api/devices").await;
    assert!(devices_ready.contains("Ready"));
}

#[tokio::test]
async fn test_daemon_mutating_endpoints_origin_and_host_validation() {
    let port = find_free_port();
    let token = format!("test_origin_token_{}", port);
    let token_clone = token.clone();
    let broker_url = "ws://127.0.0.1:8443".to_string();

    tokio::spawn(async move {
        let _ = DaemonServer::run_with_token(port, broker_url, token_clone).await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // 0. Leak Audit: Confirm that GET /api/identity NEVER leaks the local_api_token
    let id_resp = reqwest_like_get(port, "/api/identity").await;
    assert!(id_resp.contains("formatted_id"), "Identity endpoint must contain formatted_id");
    assert!(!id_resp.contains("api_token"), "SECURITY VIOLATION: Identity endpoint MUST NEVER leak api_token!");

    let test_body = serde_json::json!({
        "target_id": "123456789",
        "device_id": "test-dev"
    });

    // 1. Missing Authentication Token -> must be 401 Unauthorized
    let unauth_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[("Host", &format!("127.0.0.1:{}", port))],
    ).await;
    assert!(unauth_resp.contains("HTTP/1.1 401 Unauthorized"), "Missing token must return 401: {}", unauth_resp);
    assert!(unauth_resp.contains("Access unauthorized"), "Error must state unauthorized access");

    // 2. Bogus Authentication Token -> must be 401 Unauthorized
    let bogus_token_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Authorization", "Bearer bogus_invalid_token_999"),
        ],
    ).await;
    assert!(bogus_token_resp.contains("HTTP/1.1 401 Unauthorized"), "Invalid token must return 401: {}", bogus_token_resp);

    // 3. Substring bypass attempt: Origin: http://localhost.evil.com -> must be 403 Forbidden even with token
    let evil_sub_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Origin", "http://localhost.evil.com"),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    ).await;
    assert!(evil_sub_resp.contains("HTTP/1.1 403 Forbidden"), "localhost.evil.com must be 403: {}", evil_sub_resp);

    // 4. Substring bypass attempt with port: Origin: http://localhost.evil.com:19842 -> must be 403 Forbidden
    let evil_sub_port_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Origin", "http://localhost.evil.com:19842"),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    ).await;
    assert!(evil_sub_port_resp.contains("HTTP/1.1 403 Forbidden"), "localhost.evil.com:19842 must be 403: {}", evil_sub_port_resp);

    // 5. Substring bypass attempt: Origin: http://127.0.0.1.evil.com -> must be 403 Forbidden
    let evil_ip_sub_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Origin", "http://127.0.0.1.evil.com"),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    ).await;
    assert!(evil_ip_sub_resp.contains("HTTP/1.1 403 Forbidden"), "127.0.0.1.evil.com must be 403: {}", evil_ip_sub_resp);

    // 6. Substring bypass attempt with port: Origin: http://127.0.0.1.evil.com:19842 -> must be 403 Forbidden
    let evil_ip_sub_port_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Origin", "http://127.0.0.1.evil.com:19842"),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    ).await;
    assert!(evil_ip_sub_port_resp.contains("HTTP/1.1 403 Forbidden"), "127.0.0.1.evil.com:19842 must be 403: {}", evil_ip_sub_port_resp);

    // 7. Unexpected / Out-of-bounds port: Origin: http://localhost:99999 -> must be 403 Forbidden
    let bad_port_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Origin", "http://localhost:99999"),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    ).await;
    assert!(bad_port_resp.contains("HTTP/1.1 403 Forbidden"), "Port 99999 must be 403: {}", bad_port_resp);

    // 8. Malformed URI origins (javascript:, userinfo injection, path injection) -> must be 403 Forbidden
    let malformed_origins = [
        "javascript:alert(1)",
        "http://@evil.com",
        "http://evil.com/localhost",
        "data:text/html,evil",
    ];
    for m_origin in malformed_origins {
        let r = reqwest_like_post_with_headers(
            port,
            "/api/connect",
            &test_body,
            &[
                ("Host", &format!("127.0.0.1:{}", port)),
                ("Origin", m_origin),
                ("Authorization", &format!("Bearer {}", token)),
            ],
        ).await;
        assert!(r.contains("HTTP/1.1 403 Forbidden"), "Malformed origin '{}' must return 403: {}", m_origin, r);
    }

    // 9. DNS rebinding attacks via Host header -> must be 403 Forbidden
    let rebinding_hosts = [
        "attacker.com:19842",
        "localhost.evil.com:19842",
        "127.0.0.1.evil.com:19842",
        "localhost:99999",
    ];
    for bad_host in rebinding_hosts {
        let r = reqwest_like_post_with_headers(
            port,
            "/api/connect",
            &test_body,
            &[
                ("Host", bad_host),
                ("Authorization", &format!("Bearer {}", token)),
            ],
        ).await;
        assert!(r.contains("HTTP/1.1 403 Forbidden"), "Untrusted host header '{}' must return 403: {}", bad_host, r);
        assert!(r.contains("unauthorized Host header"), "Error message should mention unauthorized host");
    }

    // 10. Browser cross-site header (Sec-Fetch-Site: cross-site) -> must be 403 Forbidden
    let sec_site_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Sec-Fetch-Site", "cross-site"),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    ).await;
    assert!(sec_site_resp.contains("HTTP/1.1 403 Forbidden"), "Cross-site Sec-Fetch-Site must return 403: {}", sec_site_resp);

    // 11. Authorized local requests with valid token across allowed origins
    let allowed_origins = [
        format!("http://127.0.0.1:{}", port),
        "http://127.0.0.1:3000".to_string(),
        "http://localhost:19842".to_string(),
        format!("http://[::1]:{}", port), // IPv6 loopback
        "http://[::1]".to_string(),
        "tauri://localhost".to_string(),
        "null".to_string(),
    ];
    for valid_origin in allowed_origins {
        let r = reqwest_like_post_with_headers(
            port,
            "/api/connect",
            &test_body,
            &[
                ("Host", &format!("127.0.0.1:{}", port)),
                ("Origin", &valid_origin),
                ("Authorization", &format!("Bearer {}", token)),
            ],
        ).await;
        assert!(r.contains("HTTP/1.1 200 OK"), "Authorized local origin '{}' must be accepted: {}", valid_origin, r);
    }

    // 12. Token Rotation Test: POST /api/auth/rotate
    let rotate_resp = reqwest_like_post_with_headers(
        port,
        "/api/auth/rotate",
        &serde_json::json!({}),
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Origin", "http://localhost:19842"),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    ).await;
    assert!(rotate_resp.contains("HTTP/1.1 200 OK"), "Token rotation must succeed: {}", rotate_resp);
    assert!(rotate_resp.contains("successfully rotated"), "Rotation response confirmation missing");

    // After rotation: Old token must fail with 401 Unauthorized
    let old_token_retry = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Origin", "http://localhost:19842"),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    ).await;
    assert!(old_token_retry.contains("HTTP/1.1 401 Unauthorized"), "Old token must be rejected after rotation");

    // Read new token from disk (~/.usb-master-pro/daemon.token) and verify it succeeds
    let new_token = usb_master_core::daemon::read_secure_token_from_disk().unwrap();
    assert_ne!(new_token, token, "Rotated token must be different from previous token");
    let new_token_resp = reqwest_like_post_with_headers(
        port,
        "/api/connect",
        &test_body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Origin", "http://localhost:19842"),
            ("Authorization", &format!("Bearer {}", new_token)),
        ],
    ).await;
    assert!(new_token_resp.contains("HTTP/1.1 200 OK"), "New rotated token must succeed: {}", new_token_resp);
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
    reqwest_like_post_with_headers(
        port,
        path,
        body,
        &[
            ("Host", &format!("127.0.0.1:{}", port)),
            ("Authorization", &format!("Bearer {}", token)),
        ],
    )
    .await
}

async fn reqwest_like_post_with_headers(
    port: u16,
    path: &str,
    body: &serde_json::Value,
    headers: &[(&str, &str)],
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect to daemon test server");

    let payload = serde_json::to_string(body).unwrap();
    let mut header_lines = String::new();
    for (k, v) in headers {
        header_lines.push_str(&format!("{}: {}\r\n", k, v));
    }

    let req = format!(
        "POST {} HTTP/1.1\r\n{}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        path, header_lines, payload.len(), payload
    );
    stream.write_all(req.as_bytes()).await.unwrap();

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    String::from_utf8_lossy(&buf).to_string()
}


