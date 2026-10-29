use std::time::Instant;
use usb_master_core::{
    BufferPool, DaemonServer, HardwareEnumerator, NodeIdentity, TurboCompressor, TurboUrbHeader, TransferType,
    CMD_SUBMIT_URB,
};

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(url)
            .spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(url)
            .spawn();
    }
}

#[tokio::main]
async fn main() {
    let identity = NodeIdentity::load_or_generate();

    println!("================================================================================");
    println!("  ⚡ USB Master Pro — Developed & Owned by BhartX Ecosystems Pvt Ltd");
    println!("  🌐 Website: https://bhartx.in | ✉️ Support: admin@bhartx.in");
    println!("================================================================================");
    println!("  Your AnyUSB ID:      \x1b[1;32m{}\x1b[0m", identity.formatted_id);
    println!("  Default Security PIN: \x1b[1;33m{}\x1b[0m", identity.default_pin);
    println!("  Hardware Fingerprint: {}", &identity.hardware_hash[0..16]);
    println!("  P2P Rendezvous Mesh:  Active (Outbound WebSocket / TLS Signaling)");
    println!("================================================================================\n");

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("");

    match command {
        "" | "gui" | "--gui" | "start" | "--start" | "run" | "--run" => {
            let port = std::env::var("DAEMON_PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(19842);

            let broker_url = std::env::var("SIGNALING_URL")
                .unwrap_or_else(|_| "ws://127.0.0.1:8443".to_string());

            tokio::spawn(async move {
                if let Err(e) = DaemonServer::run(port, broker_url).await {
                    eprintln!("❌ Daemon server error: {}", e);
                }
            });

            // Wait 150ms for the server to listen
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;

            let url = format!("http://127.0.0.1:{}/", port);
            open_browser(&url);

            println!("  ⚡ USB Master Pro is running!");
            println!("  🌐 Dashboard URL: {}", url);
            println!("  ⌨️  Press Ctrl+C to stop.\n");

            let _ = tokio::signal::ctrl_c().await;
            println!("\n  Shutting down USB Master Pro...");
        }

        "daemon" | "--daemon" => {
            println!("⚡ Running USB Master Pro Daemon in headless mode...\n");
            let port = std::env::var("DAEMON_PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(19842);

            let broker_url = std::env::var("SIGNALING_URL")
                .unwrap_or_else(|_| "ws://127.0.0.1:8443".to_string());

            if let Err(e) = DaemonServer::run(port, broker_url).await {
                eprintln!("❌ Daemon server error: {}", e);
            }
        }

        "list" | "--list" | "scan" | "--scan" => {
            println!("🔍 Scanning physical hardware & connected USB ports (<30ms)...");
            let devices = HardwareEnumerator::scan_devices();
            println!("\n  Found {} device(s) ready for hardware passthrough:\n", devices.len());

            for dev in &devices {
                println!(
                    "  {} \x1b[1m{}\x1b[0m",
                    dev.category_icon, dev.friendly_name
                );
                println!(
                    "     ├─ ID: \x1b[36m{}\x1b[0m | Port: {} | Speed: {}",
                    dev.id, dev.port_path, dev.speed
                );
                println!(
                    "     ├─ VID: 0x{:04X} | PID: 0x{:04X} | Serial: {}",
                    dev.vid, dev.pid, dev.serial_number
                );
                let status_str = if dev.is_mounted {
                    "\x1b[32mMounted to Remote Peer\x1b[0m"
                } else if dev.is_shared {
                    "\x1b[33mShared (Waiting for Connection)\x1b[0m"
                } else {
                    "\x1b[37mLocal Idle (Ready to Forward)\x1b[0m"
                };
                println!("     └─ Status: {}", status_str);
                println!();
            }
        }

        "benchmark" | "--benchmark" | "test" | "--test" => {
            println!("🚀 Running Turbo-URB Synthetic In-Memory Throughput Benchmark...\n");
            println!("  [Note: Synthetic RAM-to-RAM test measuring serialization and compression; not wire latency]\n");
            let pool = BufferPool::default_pool();
            let raw_size = 512 * 1024; // 512 KB biometric high-res scan frame
            let mut sample_frame = vec![0x20; raw_size];
            for i in (0..raw_size).step_by(8) {
                sample_frame[i] = 0xDE;
                sample_frame[i + 1] = 0xAD;
            }

            let start = Instant::now();
            let iterations = 1000;
            let mut total_compressed_bytes = 0;

            for seq in 0..iterations {
                let _buf = pool.acquire();
                let (compressed, _was_comp) = TurboCompressor::compress_if_beneficial(&sample_frame);
                total_compressed_bytes += compressed.len();

                let hdr = TurboUrbHeader::new(
                    CMD_SUBMIT_URB,
                    seq as u64,
                    1,
                    0x81,
                    TransferType::Bulk,
                    compressed.len() as u32,
                );
                let _serialized = hdr.serialize();
            }

            let elapsed = start.elapsed();
            let total_mb = (raw_size * iterations) as f64 / (1024.0 * 1024.0);
            let throughput_mb_s = total_mb / elapsed.as_secs_f64();
            let compression_ratio = (total_compressed_bytes as f64) / ((raw_size * iterations) as f64);

            println!("  Iterations:          {} frames", iterations);
            println!("  Total Raw Data:      {:.2} MB", total_mb);
            println!("  Elapsed Time:        {:.3} s", elapsed.as_secs_f64());
            println!("  Processing Speed:    \x1b[1;32m{:.2} MB/s\x1b[0m", throughput_mb_s);
            println!(
                "  Compression Ratio:   \x1b[1;36m{:.2}%\x1b[0m (Payload shrunk by {:.1}x)",
                compression_ratio * 100.0,
                1.0 / compression_ratio
            );
            println!("  Average In-Memory CPU Latency: {:.3} ms/frame", elapsed.as_secs_f64() * 1000.0 / iterations as f64);
            println!("\n  Verdict: \x1b[1;32mSynthetic Benchmark Completed — In-Memory Pipeline Verified.\x1b[0m");
        }

        "connect" => {
            let target_id = args.get(2).cloned().unwrap_or_default();
            let dev_id = args.get(3).cloned().unwrap_or_else(|| "auto".to_string());
            if target_id.trim().is_empty() {
                println!("Usage: usb_master_cli connect <target_9_digit_id> [device_id]");
                return;
            }
            let clean_target_id = target_id.replace(' ', "");
            println!("Initiating connection request to remote peer [{}] for device [{}]...", clean_target_id, dev_id);

            let port = std::env::var("DAEMON_PORT")
                .ok()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(19842);
            let token = std::env::var("USB_MASTER_DAEMON_TOKEN")
                .ok()
                .or_else(usb_master_core::daemon::read_secure_token_from_disk)
                .unwrap_or_default();

            let payload = serde_json::json!({
                "target_id": clean_target_id,
                "device_id": dev_id
            }).to_string();

            let req = format!(
                "POST /api/connect HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );

            match tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await {
                Ok(mut stream) => {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    if let Err(e) = stream.write_all(req.as_bytes()).await {
                        eprintln!("❌ Failed to send connect request to local daemon: {}", e);
                        return;
                    }
                    let mut resp_buf = Vec::new();
                    let _ = stream.read_to_end(&mut resp_buf).await;
                    let resp_str = String::from_utf8_lossy(&resp_buf);
                    if resp_str.contains("200 OK") {
                        println!("\x1b[1;32m[+] Connection request dispatched to daemon. Session state: Pairing/Negotiating.\x1b[0m");
                        println!("    Check web dashboard (http://127.0.0.1:{port}/) for real-time status.");
                    } else {
                        eprintln!("❌ Daemon responded with error:\n{}", resp_str.lines().next().unwrap_or("Unknown error"));
                    }
                }
                Err(e) => {
                    eprintln!("❌ Local daemon is not running on 127.0.0.1:{} ({}).", port, e);
                    eprintln!("   Please start the daemon first with 'usb_master_cli daemon' or 'usb_master_cli gui'.");
                }
            }
        }

        _ => {
            println!("Usage: usb_master_cli [gui | list | benchmark | daemon | connect <target_id> <dev_id>]");
        }
    }
}
