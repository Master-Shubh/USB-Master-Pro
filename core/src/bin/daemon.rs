//! USB Master Pro — Production IPC Daemon Binary
//! Runs embedded HTTP/WS server on 127.0.0.1:19842 and connects to Signaling Broker.

use usb_master_core::daemon::DaemonServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("================================================================================");
    println!("  ⚡ USB Master Pro — Embedded IPC Daemon & Mesh Fabric");
    println!("  🏢 BhartX Ecosystems Pvt Ltd | https://bhartx.in");
    println!("================================================================================");

    let port = std::env::var("DAEMON_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(19842);

    let broker_url = std::env::var("SIGNALING_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:8443".to_string());

    DaemonServer::run(port, broker_url).await
}
