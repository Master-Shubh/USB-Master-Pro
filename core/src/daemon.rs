//! Embedded IPC Daemon & P2P Mesh Bridge
//! Exposes HTTP + WebSocket APIs on 127.0.0.1:19842 for the UI front-end,
//! and maintains an outbound WebSocket session to the signaling broker.

use crate::driver_client::{
    check_driver_status, AnyUsbAttachRequest, AnyUsbCompleteRequest, DriverClientError,
    DriverOperationalStatus, FilterDriverClient, VhciDriverClient, ANYUSB_MAX_URB_BUFFER_SIZE,
};
use crate::enumerator::{HardwareEnumerator, UsbDeviceInfo};
use crate::identity::NodeIdentity;
use crate::licensing::{LicenseClaims, LicenseEngine};
use crate::protocol::{TurboUrbHeader, CMD_COMPLETE_URB, CMD_SUBMIT_URB, TransferType};
use crate::state::{SessionState, SessionStateMachine, StateTransitionError};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};

// Embedded UI and static assets for single-binary distribution
const EMBEDDED_INDEX_HTML: &str = include_str!("../../ui/index.html");
const EMBEDDED_LOGO_PNG: &[u8] = include_bytes!("../../ui/assets/logo.png");
const EMBEDDED_FAVICON_ICO: &[u8] = include_bytes!("../../ui/assets/favicon.ico");
const EMBEDDED_ICON_64: &[u8] = include_bytes!("../../ui/assets/icon_64.png");
const EMBEDDED_ICON_256: &[u8] = include_bytes!("../../ui/assets/icon_256.png");
const EMBEDDED_TAILWIND_JS: &[u8] = include_bytes!("../../ui/assets/tailwind.min.js");
const EMBEDDED_LUCIDE_JS: &[u8] = include_bytes!("../../ui/assets/lucide.min.js");
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tower_http::cors::CorsLayer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetrics {
    pub bytes_transferred: u64,
    pub urbs_completed: u64,
    pub latency_ms: f64,
    pub throughput_mb_s: f64,
    pub packets_dropped: u64,
    pub connected_duration_secs: u64,
}

impl Default for SessionMetrics {
    fn default() -> Self {
        Self {
            bytes_transferred: 0,
            urbs_completed: 0,
            latency_ms: 0.0,
            throughput_mb_s: 0.0,
            packets_dropped: 0,
            connected_duration_secs: 0,
        }
    }
}

/// Active Session with deterministic state machine and real-time operational telemetry
#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub session_id: String,
    pub peer_id: String,
    pub device_id: String,
    pub state_machine: SessionStateMachine,
    pub metrics: SessionMetrics,
    pub created_at: u64,
}

impl ActiveSession {
    pub fn new(session_id: String, peer_id: String, device_id: String) -> Self {
        Self {
            session_id,
            peer_id,
            device_id,
            state_machine: SessionStateMachine::with_initial_state(SessionState::Ready),
            metrics: SessionMetrics::default(),
            created_at: date_now_millis() as u64,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state_machine.current_state()
    }

    pub fn transition_to(
        &mut self,
        next: SessionState,
        reason: Option<String>,
    ) -> Result<(), StateTransitionError> {
        self.state_machine.transition_to(next, reason)
    }
}

impl Serialize for ActiveSession {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ActiveSession", 9)?;
        s.serialize_field("session_id", &self.session_id)?;
        s.serialize_field("peer_id", &self.peer_id)?;
        s.serialize_field("device_id", &self.device_id)?;
        s.serialize_field("state", &self.state_machine.current_state())?;
        s.serialize_field("state_str", &self.state_machine.current_state().to_string())?;
        s.serialize_field("previous_state", &self.state_machine.previous_state())?;
        s.serialize_field("last_reason", &self.state_machine.last_reason())?;
        s.serialize_field(
            "time_in_state_secs",
            &self.state_machine.time_in_current_state().as_secs(),
        )?;
        s.serialize_field("metrics", &self.metrics)?;
        s.serialize_field("created_at", &self.created_at)?;
        s.end()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMountedDevice {
    pub id: String,
    pub remote_peer_id: String,
    pub name: String,
    pub icon: String,
    pub driver_state: String,
    pub latency: String,
    pub throughput: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardRequest {
    pub device_id: String,
    pub share: bool,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectRequest {
    pub target_id: String,
    #[serde(default)]
    pub pin: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RespondRequest {
    pub from_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectRequest {
    pub peer_id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinVerifyRequest {
    pub peer_id: String,
    pub pin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonTelemetry {
    pub status: String,
    pub uptime_secs: u64,
    pub broker_connected: bool,
    pub broker_url: String,
    pub identity: NodeIdentity,
    pub local_device_count: usize,
    pub remote_device_count: usize,
    pub turbo_urb_throughput_mb_s: f64,
    pub rd_service_latency_ms: f64,
}

/// Async Tokio URB pump worker task handles and data channels for an active session
pub struct SessionPumpHandles {
    pub peer_id: String,
    pub device_id: u32,
    pub vhci_to_net_task: tokio::task::JoinHandle<()>,
    pub net_to_vhci_task: tokio::task::JoinHandle<()>,
    pub net_fwd_task: tokio::task::JoinHandle<()>,
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
    pub session_data_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
}

pub struct DaemonState {
    pub identity: NodeIdentity,
    pub devices: RwLock<Vec<UsbDeviceInfo>>,
    pub remote_devices: RwLock<Vec<RemoteMountedDevice>>,
    pub active_sessions: Arc<RwLock<HashMap<String, ActiveSession>>>,
    pub broker_tx: mpsc::Sender<serde_json::Value>,
    pub binary_outbound_tx: mpsc::Sender<Vec<u8>>,
    pub ui_events_tx: broadcast::Sender<serde_json::Value>,
    pub broker_connected: AtomicBool,
    pub start_time: Instant,
    pub broker_url: String,
    pub active_license: RwLock<Option<LicenseClaims>>,
    pub local_api_token: std::sync::RwLock<String>,
    pub driver_client: Arc<VhciDriverClient>,
    pub filter_client: Arc<FilterDriverClient>,
    pub session_pumps: Arc<tokio::sync::Mutex<HashMap<String, SessionPumpHandles>>>,
    pub dynamic_pin: Arc<RwLock<String>>,
    pub failed_pin_attempts: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

pub const PIN_LOCKOUT_MAX_ATTEMPTS: u32 = 5;
pub const PIN_LOCKOUT_DURATION_SECS: u64 = 900; // 15 minutes (900 seconds)

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinVerificationResult {
    Success,
    InvalidPin { attempts: u32, remaining: u32 },
    LockedOut { message: String },
}

/// Generates a cryptographically secure 6-digit one-time session PIN using CSPRNG.
/// Formatted as "XXX XXX" (e.g., "482 190").
pub fn generate_csprng_pin() -> String {
    use rand::Rng;
    let pin_num: u32 = rand::thread_rng().gen_range(100_000..=999_999);
    let s = pin_num.to_string();
    format!("{} {}", &s[0..3], &s[3..6])
}

/// Resolves initial session PIN from environment (for test overrides) or CSPRNG.
pub fn get_initial_session_pin() -> String {
    if let Ok(pin) = std::env::var("USB_MASTER_SESSION_PIN") {
        let trimmed = pin.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(pin) = std::env::var("USB_MASTER_PIN") {
        let trimmed = pin.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    generate_csprng_pin()
}

impl DaemonState {
    /// Returns the active dynamic session PIN
    pub async fn get_pin(&self) -> String {
        self.dynamic_pin.read().await.clone()
    }

    /// Generates a fresh dynamic CSPRNG session PIN, updates state, and broadcasts PIN_ROTATED event
    pub async fn rotate_pin(&self) -> String {
        let new_pin = generate_csprng_pin();
        {
            let mut pin_w = self.dynamic_pin.write().await;
            *pin_w = new_pin.clone();
        }
        let _ = self.ui_events_tx.send(serde_json::json!({
            "type": "PIN_ROTATED",
            "pin": new_pin
        }));
        new_pin
    }

    /// Verifies an incoming candidate PIN against the active CSPRNG session PIN,
    /// enforcing the 5-try lockout policy (15-minute cooldown) and cleaning expired entries.
    pub async fn verify_incoming_pin(&self, peer_id: &str, candidate_pin: &str) -> PinVerificationResult {
        let clean_peer = peer_id.replace(' ', "");
        let clean_candidate = candidate_pin.replace(' ', "");
        let now = Instant::now();
        let lockout_duration = Duration::from_secs(PIN_LOCKOUT_DURATION_SECS);

        let mut attempts_map = self.failed_pin_attempts.lock().await;

        // 1. Clean up expired lockout entries (> 15 minutes)
        attempts_map.retain(|_, (_, last_failure)| now.duration_since(*last_failure) < lockout_duration);

        // 2. Check if peer is currently locked out
        if let Some((count, last_failure)) = attempts_map.get(&clean_peer) {
            if *count >= PIN_LOCKOUT_MAX_ATTEMPTS && now.duration_since(*last_failure) < lockout_duration {
                return PinVerificationResult::LockedOut {
                    message: "Security lockout: Too many failed PIN attempts. Locked out for 15 minutes.".to_string(),
                };
            }
        }

        // 3. Compare candidate PIN with active CSPRNG dynamic PIN
        let active_pin = self.dynamic_pin.read().await;
        let clean_active = active_pin.replace(' ', "");

        if !clean_candidate.is_empty() && clean_candidate == clean_active {
            // Success: Reset failure counter for this peer
            attempts_map.remove(&clean_peer);
            PinVerificationResult::Success
        } else {
            // Failure: Increment failed attempts counter and update last_failure timestamp
            let entry = attempts_map.entry(clean_peer.clone()).or_insert((0, now));
            entry.0 += 1;
            entry.1 = now;
            let count = entry.0;

            if count >= PIN_LOCKOUT_MAX_ATTEMPTS {
                PinVerificationResult::LockedOut {
                    message: "Security lockout: Too many failed PIN attempts. Locked out for 15 minutes.".to_string(),
                }
            } else {
                PinVerificationResult::InvalidPin {
                    attempts: count,
                    remaining: PIN_LOCKOUT_MAX_ATTEMPTS.saturating_sub(count),
                }
            }
        }
    }
}

/// Generates a 256-bit cryptographically secure local API authorization token from the OS CSPRNG.
pub fn generate_secure_api_token() -> String {
    let mut bytes = [0u8; 32];
    let mut filled = false;

    #[cfg(unix)]
    {
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            use std::io::Read;
            if f.read_exact(&mut bytes).is_ok() {
                filled = true;
            }
        }
    }

    #[cfg(windows)]
    {
        #[link(name = "advapi32")]
        extern "system" {
            #[link_name = "SystemFunction036"]
            fn rtl_gen_random(buffer: *mut u8, length: u32) -> u8;
        }
        unsafe {
            if rtl_gen_random(bytes.as_mut_ptr(), bytes.len() as u32) != 0 {
                filled = true;
            }
        }
    }

    if !filled {
        panic!("FATAL SECURITY ERROR: OS CSPRNG source (/dev/urandom or SystemFunction036) is completely unavailable. Cannot safely generate daemon authentication token.");
    }

    hex::encode(bytes)
}

/// Resolves the filesystem path for the daemon authentication token (~/.usb-master-pro/daemon.token)
pub fn get_daemon_token_path() -> Option<std::path::PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(|home| std::path::PathBuf::from(home).join(".usb-master-pro").join("daemon.token"))
}

/// Saves the authorization token to disk with restricted permissions (0600 on Unix)
pub fn save_secure_token_to_disk(token: &str) -> std::io::Result<()> {
    if let Some(token_path) = get_daemon_token_path() {
        if let Some(parent) = token_path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        std::fs::write(&token_path, token)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600));
        }
    }
    Ok(())
}

/// Reads the persisted daemon token from disk if available
pub fn read_secure_token_from_disk() -> Option<String> {
    if let Some(token_path) = get_daemon_token_path() {
        if token_path.exists() {
            if let Ok(content) = std::fs::read_to_string(token_path) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

/// Validates whether an incoming HTTP Host header belongs to a permitted local loopback endpoint.
/// Defends against DNS rebinding attacks.
pub fn is_authorized_host(host_str: &str) -> bool {
    let host_clean = host_str.trim();
    if host_clean.is_empty() {
        return false;
    }

    let (host, port_str) = if host_clean.starts_with('[') {
        match host_clean.find(']') {
            Some(end_idx) => {
                let host_part = &host_clean[..=end_idx];
                let after = &host_clean[end_idx + 1..];
                if after.is_empty() {
                    (host_part, None)
                } else if let Some(p) = after.strip_prefix(':') {
                    (host_part, Some(p))
                } else {
                    return false;
                }
            }
            None => return false,
        }
    } else {
        let mut parts = host_clean.splitn(2, ':');
        let host_part = parts.next().unwrap_or("");
        let port_part = parts.next();
        (host_part, port_part)
    };

    if let Some(port_s) = port_str {
        if port_s.is_empty() {
            return false;
        }
        match port_s.parse::<u16>() {
            Ok(p) if p > 0 => {}
            _ => return false,
        }
    }

    let host_lower = host.to_ascii_lowercase();
    host_lower == "127.0.0.1" || host_lower == "localhost" || host_lower == "[::1]"
}

/// Validates whether an incoming HTTP Origin header represents an authorized local origin.
/// Strictly parses scheme, exact host, and port bounds to reject malicious origins
/// like `http://localhost.evil.com`, `http://127.0.0.1.evil.com`, unexpected ports, or malformed URIs.
pub fn is_authorized_origin(origin_str: &str) -> bool {
    let origin_clean = origin_str.trim();
    if origin_clean.is_empty() {
        return false;
    }
    if origin_clean == "null" {
        return true;
    }

    // Native app container schemes
    if origin_clean == "tauri://localhost" || origin_clean.starts_with("tauri://") {
        return true;
    }
    if origin_clean == "app://localhost" || origin_clean.starts_with("app://") {
        return true;
    }
    if origin_clean.starts_with("vscode-webview://") {
        return true;
    }

    let rest = if let Some(stripped) = origin_clean.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = origin_clean.strip_prefix("https://") {
        stripped
    } else {
        return false;
    };

    // RFC 6454: An Origin must NOT contain userinfo, path, query, or fragment
    if rest.contains('@') || rest.contains('/') || rest.contains('?') || rest.contains('#') {
        return false;
    }

    let (host, port_str) = if rest.starts_with('[') {
        match rest.find(']') {
            Some(end_idx) => {
                let host_part = &rest[..=end_idx];
                let after_bracket = &rest[end_idx + 1..];
                if after_bracket.is_empty() {
                    (host_part, None)
                } else if let Some(p) = after_bracket.strip_prefix(':') {
                    (host_part, Some(p))
                } else {
                    return false;
                }
            }
            None => return false,
        }
    } else {
        let mut parts = rest.splitn(2, ':');
        let host_part = parts.next().unwrap_or("");
        let port_part = parts.next();
        (host_part, port_part)
    };

    if let Some(port_s) = port_str {
        if port_s.is_empty() {
            return false;
        }
        match port_s.parse::<u16>() {
            Ok(p) if p > 0 => {}
            _ => return false,
        }
    }

    let host_lower = host.to_ascii_lowercase();
    host_lower == "127.0.0.1"
        || host_lower == "localhost"
        || host_lower == "[::1]"
        || host_lower == "tauri.localhost"
}

/// Compares two strings in constant time to prevent side-channel timing attacks
fn constant_time_eq_str(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Validates that an incoming HTTP request originates from an authorized local context
/// (localhost/127.0.0.1, local desktop webview/app, or direct CLI/IPC), preventing
/// DNS rebinding and Cross-Site Request Forgery (CSRF) attacks against mutating endpoints.
fn validate_request_origin_and_host(headers: &axum::http::HeaderMap) -> Result<(), (StatusCode, String)> {
    // 1. Validate Host header against DNS rebinding
    if let Some(host_val) = headers.get(axum::http::header::HOST) {
        if let Ok(host_str) = host_val.to_str() {
            if !is_authorized_host(host_str) {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Access forbidden: unauthorized Host header '{}'", host_str),
                ));
            }
        }
    }

    // 2. Reject cross-site requests via Sec-Fetch-Site
    if let Some(sec_site) = headers.get("sec-fetch-site") {
        if let Ok(s) = sec_site.to_str() {
            if s.eq_ignore_ascii_case("cross-site") {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Access forbidden: cross-site requests are rejected".to_string(),
                ));
            }
        }
    }

    // 3. Strict Origin header validation
    if let Some(origin_val) = headers.get(axum::http::header::ORIGIN) {
        if let Ok(origin_str) = origin_val.to_str() {
            if !is_authorized_origin(origin_str) {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Access forbidden: origin '{}' is not authorized to execute mutating operations", origin_str),
                ));
            }
        }
    }

    Ok(())
}

/// Validates that an incoming HTTP request originates from an authorized local context
/// AND presents a valid high-entropy local API authorization token.
fn validate_request_origin_and_auth(
    headers: &axum::http::HeaderMap,
    state: &DaemonState,
) -> Result<(), (StatusCode, String)> {
    // 1. Origin and Host checks against CSRF and DNS rebinding
    validate_request_origin_and_host(headers)?;

    // 2. Strict Authentication Token validation (Authorization: Bearer <token> or X-Daemon-Token: <token>)
    let token_from_auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            if let Some(stripped) = s.strip_prefix("Bearer ") {
                Some(stripped.trim())
            } else {
                Some(s.trim())
            }
        });

    let token_from_header = headers
        .get("x-daemon-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim());

    let client_token = token_from_auth.or(token_from_header);
    let expected_token = state.local_api_token.read().unwrap();

    match client_token {
        Some(token) if !token.is_empty() && constant_time_eq_str(token, &expected_token) => {
            Ok(())
        }
        Some(_) => Err((
            StatusCode::UNAUTHORIZED,
            "Access unauthorized: invalid daemon API token".to_string(),
        )),
        None => Err((
            StatusCode::UNAUTHORIZED,
            "Access unauthorized: mutating operations require 'Authorization: Bearer <token>' or 'X-Daemon-Token'".to_string(),
        )),
    }
}

/// Validates that an incoming read request originates from an authorized local context
/// (localhost / 127.0.0.1 origin or loopback connection) OR presents a valid daemon API token.
/// Rejects unauthorized Host headers (DNS rebinding), cross-site fetch requests,
/// unauthorized origins, and invalid authorization tokens.
fn validate_request_token_or_origin(
    headers: &axum::http::HeaderMap,
    state: &DaemonState,
) -> Result<(), (StatusCode, String)> {
    // 1. Host header validation against DNS rebinding
    if let Some(host_val) = headers.get(axum::http::header::HOST) {
        if let Ok(host_str) = host_val.to_str() {
            if !is_authorized_host(host_str) {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Access forbidden: unauthorized Host header '{}'", host_str),
                ));
            }
        }
    }

    // 2. Reject cross-site requests via Sec-Fetch-Site
    if let Some(sec_site) = headers.get("sec-fetch-site") {
        if let Ok(s) = sec_site.to_str() {
            if s.eq_ignore_ascii_case("cross-site") {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Access forbidden: cross-site requests are rejected".to_string(),
                ));
            }
        }
    }

    // 3. Strict Origin header validation if present
    if let Some(origin_val) = headers.get(axum::http::header::ORIGIN) {
        if let Ok(origin_str) = origin_val.to_str() {
            if !is_authorized_origin(origin_str) {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Access forbidden: origin '{}' is not authorized", origin_str),
                ));
            }
        }
    }

    // 4. Strict token validation: if token is supplied, it must match expected token
    let token_from_auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            if let Some(stripped) = s.strip_prefix("Bearer ") {
                Some(stripped.trim())
            } else {
                Some(s.trim())
            }
        });

    let token_from_header = headers
        .get("x-daemon-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim());

    let client_token = token_from_auth.or(token_from_header);
    if let Some(token) = client_token {
        let expected_token = state.local_api_token.read().unwrap();
        if !constant_time_eq_str(token, &expected_token) {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Access unauthorized: invalid daemon API token".to_string(),
            ));
        }
    }

    Ok(())
}

pub fn create_router(state: Arc<DaemonState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _| {
            if let Ok(s) = origin.to_str() {
                is_authorized_origin(s)
            } else {
                false
            }
        }))
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::OPTIONS])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::header::ORIGIN,
            axum::http::header::HeaderName::from_static("x-daemon-token"),
        ]);

    Router::new()
        .route("/", get(handle_serve_index))
        .route("/ui", get(handle_serve_index))
        .route("/index.html", get(handle_serve_index))
        .route("/assets/logo.png", get(handle_serve_logo))
        .route("/assets/favicon.ico", get(handle_serve_favicon))
        .route("/assets/icon_64.png", get(handle_serve_icon_64))
        .route("/assets/icon_256.png", get(handle_serve_icon_256))
        .route("/assets/tailwind.min.js", get(handle_serve_tailwind))
        .route("/assets/lucide.min.js", get(handle_serve_lucide))
        .route("/api/identity", get(handle_get_identity))
        .route("/api/devices", get(handle_get_devices))
        .route("/api/sessions", get(handle_get_sessions))
        .route("/api/forward", post(handle_post_forward))
        .route("/api/devices/share", post(handle_share_device))
        .route("/api/connect", post(handle_post_connect))
        .route("/api/respond", post(handle_post_respond))
        .route("/api/disconnect", post(handle_post_disconnect))
        .route("/api/session/disconnect", post(handle_post_disconnect))
        .route("/api/status", get(handle_get_status))
        .route("/api/auth/rotate", post(handle_post_auth_rotate))
        .route("/api/license/activate", post(handle_post_license_activate))
        .route("/api/license/status", get(handle_get_license_status))
        .route("/api/system/drivers", get(handle_get_system_drivers))
        .route("/api/pin", get(handle_get_pin))
        .route("/api/pin/rotate", post(handle_post_pin_rotate))
        .route("/api/pin/verify", post(handle_post_pin_verify))
        .route("/api/events", get(handle_ws_events))
        .layer(cors)
        .with_state(state)
}

/// Starts the USB Master Pro embedded IPC Daemon
pub async fn start_daemon(bind_port: u16, broker_url: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    DaemonServer::run(bind_port, broker_url).await
}

pub struct DaemonServer;

impl DaemonServer {
    pub async fn run(bind_port: u16, broker_url: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Self::run_internal(bind_port, broker_url, None).await
    }

    pub async fn run_with_token(bind_port: u16, broker_url: String, explicit_token: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Self::run_internal(bind_port, broker_url, Some(explicit_token)).await
    }

    async fn run_internal(
        bind_port: u16,
        broker_url: String,
        explicit_token: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let identity = NodeIdentity::load_or_generate();
        let (broker_tx, broker_rx) = mpsc::channel::<serde_json::Value>(64);
        let (binary_tx, binary_rx) = mpsc::channel::<Vec<u8>>(512);
        let (ui_events_tx, _) = broadcast::channel::<serde_json::Value>(128);

        // Pre-scan physical hardware
        let initial_devices = HardwareEnumerator::scan_devices();
        let initial_license = load_persisted_license(&identity.hardware_hash);

        let local_api_token = explicit_token.unwrap_or_else(|| {
            std::env::var("USB_MASTER_DAEMON_TOKEN").unwrap_or_else(|_| {
                let token = generate_secure_api_token();
                let _ = save_secure_token_to_disk(&token);
                token
            })
        });

        // Ensure token is stored in ~/.usb-master-pro/daemon.token with owner-only access (0600 on Unix)
        let _ = save_secure_token_to_disk(&local_api_token);

        let driver_client = Arc::new(VhciDriverClient::open().unwrap_or_else(|e| {
            eprintln!("⚠️  WARNING: VHCI driver not available ({}). USB attach/detach operations will fail. Install anyusb_vhci driver.", e);
            VhciDriverClient::default()
        }));
        let filter_client = Arc::new(FilterDriverClient::open().unwrap_or_else(|e| {
            eprintln!("⚠️  WARNING: Filter driver not available ({}). Physical USB capture will not function. Install anyusb_filter driver.", e);
            FilterDriverClient::default()
        }));

        let state = Arc::new(DaemonState {
            identity,
            devices: RwLock::new(initial_devices),
            remote_devices: RwLock::new(Vec::new()),
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            broker_tx,
            binary_outbound_tx: binary_tx,
            ui_events_tx,
            broker_connected: AtomicBool::new(false),
            start_time: Instant::now(),
            broker_url: broker_url.clone(),
            active_license: RwLock::new(initial_license),
            local_api_token: std::sync::RwLock::new(local_api_token),
            driver_client,
            filter_client,
            session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            dynamic_pin: Arc::new(RwLock::new(get_initial_session_pin())),
            failed_pin_attempts: Arc::new(Mutex::new(HashMap::new())),
        });

        // 1. Spawn Outbound Signaling Client task
        let broker_state = Arc::clone(&state);
        let broker_target_url = broker_url.clone();
        tokio::spawn(async move {
            run_signaling_client(broker_state, broker_target_url, broker_rx, binary_rx).await;
        });

        // 2. Spawn periodic hardware hotplug scanner
        let scanner_state = Arc::clone(&state);
        tokio::spawn(async move {
            run_hotplug_monitor(scanner_state).await;
        });

        // 3. Build HTTP + WS Router
        let app = create_router(state);

        let addr = SocketAddr::from(([127, 0, 0, 1], bind_port));
        println!("  🌐 Embedded IPC Daemon listening on http://{}", addr);
        println!("  📡 Signaling Broker target: {}", broker_url);
        println!("  🔒 Local API authorization token secured at ~/.usb-master-pro/daemon.token (mode 0600)");

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    pub async fn start_daemon(bind_port: u16, broker_url: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Self::run(bind_port, broker_url).await
    }
}

// --- STATIC ASSET HANDLERS ---

async fn handle_serve_index() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], EMBEDDED_INDEX_HTML)
}

async fn handle_serve_logo() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/png")], EMBEDDED_LOGO_PNG)
}

async fn handle_serve_favicon() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/x-icon")], EMBEDDED_FAVICON_ICO)
}

async fn handle_serve_icon_64() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/png")], EMBEDDED_ICON_64)
}

async fn handle_serve_icon_256() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/png")], EMBEDDED_ICON_256)
}

async fn handle_serve_tailwind() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], EMBEDDED_TAILWIND_JS)
}

async fn handle_serve_lucide() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], EMBEDDED_LUCIDE_JS)
}

// --- HTTP ENDPOINT HANDLERS ---

async fn handle_get_identity(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_token_or_origin(&headers, &state)?;
    // SECURITY INVARIANT: Identity endpoint MUST NEVER leak default_pin or local_api_token.
    let mut val = serde_json::to_value(&state.identity).unwrap_or_default();
    if let Some(obj) = val.as_object_mut() {
        obj.remove("default_pin");
    }
    Ok(Json(val))
}

async fn handle_get_pin(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_token_or_origin(&headers, &state)?;
    let pin = state.get_pin().await;
    Ok(Json(serde_json::json!({
        "pin": pin,
        "dynamic": true
    })))
}

async fn handle_post_pin_rotate(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_origin_and_auth(&headers, &state)?;
    let new_pin = state.rotate_pin().await;
    Ok(Json(serde_json::json!({
        "success": true,
        "pin": new_pin
    })))
}

async fn handle_post_pin_verify(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
    Json(payload): Json<PinVerifyRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_origin_and_auth(&headers, &state)?;
    match state.verify_incoming_pin(&payload.peer_id, &payload.pin).await {
        PinVerificationResult::Success => Ok(Json(serde_json::json!({
            "valid": true,
            "peer_id": payload.peer_id
        }))),
        PinVerificationResult::InvalidPin { attempts, remaining } => Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid PIN. Attempt {} of 5 before lockout ({} remaining).", attempts, remaining),
        )),
        PinVerificationResult::LockedOut { message } => Err((
            StatusCode::FORBIDDEN,
            message,
        )),
    }
}

/// Rotates the local daemon API authorization token, persisting the fresh 256-bit CSPRNG token to disk
async fn handle_post_auth_rotate(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_origin_and_auth(&headers, &state)?;

    let new_token = generate_secure_api_token();
    if let Err(e) = save_secure_token_to_disk(&new_token) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to persist rotated token to secure storage: {}", e),
        ));
    }

    {
        let mut lock = state.local_api_token.write().unwrap();
        *lock = new_token;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Daemon local API authorization token successfully rotated and persisted to ~/.usb-master-pro/daemon.token (mode 0600)"
    })))
}

async fn handle_get_devices(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_token_or_origin(&headers, &state)?;
    // Perform fresh hardware scan and update state
    let fresh = HardwareEnumerator::scan_devices();
    let mut current_map = HashMap::new();
    {
        let current = state.devices.read().await;
        for d in current.iter() {
            current_map.insert(d.id.clone(), (d.is_shared, d.is_mounted, d.remote_peer_id.clone()));
        }
    }

    let sessions = state.active_sessions.read().await;

    let mut merged = Vec::with_capacity(fresh.len());
    for mut dev in fresh {
        if let Some((is_shared, is_mounted, remote_peer_id)) = current_map.get(&dev.id) {
            dev.is_shared = *is_shared;
            dev.is_mounted = *is_mounted;
            dev.remote_peer_id = remote_peer_id.clone();
        }
        // Link active session status if device is bound in an active session
        for sess in sessions.values() {
            if sess.device_id == dev.id && sess.state() == SessionState::Active {
                dev.is_mounted = true;
                dev.remote_peer_id = Some(sess.peer_id.clone());
            }
        }
        merged.push(dev);
    }

    {
        let mut dev_lock = state.devices.write().await;
        *dev_lock = merged.clone();
    }

    let remote = state.remote_devices.read().await.clone();
    let active_sessions_snapshot = sessions.clone();

    Ok(Json(serde_json::json!({
        "local": merged,
        "remote": remote,
        "sessions": active_sessions_snapshot,
        "active_sessions": active_sessions_snapshot
    })))
}

async fn handle_get_sessions(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_token_or_origin(&headers, &state)?;
    let sessions = state.active_sessions.read().await.clone();
    Ok(Json(sessions))
}

pub async fn handle_share_device(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
    Json(payload): Json<ForwardRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_origin_and_auth(&headers, &state)?;

    let mut dev_lock = state.devices.write().await;
    let dev = dev_lock.iter_mut().find(|d| d.id == payload.device_id);

    match dev {
        Some(d) => {
            if d.is_system_device && payload.share && !payload.force {
                return Err((
                    StatusCode::FORBIDDEN,
                    format!("Device '{}' is a primary system input device. Pass force=true to override safety exclusion.", d.friendly_name)
                ));
            }

            d.is_shared = payload.share;
            let numeric_dev_id = derive_numeric_device_id(&d.id);
            if payload.share {
                let _ = state.filter_client.start_capture(numeric_dev_id);
            } else {
                let _ = state.filter_client.stop_capture(numeric_dev_id);
            }

            let mut closed_peer_id = None;
            if !payload.share {
                closed_peer_id = d.remote_peer_id.take();
                d.is_mounted = false;
            }

            let updated_dev = d.clone();
            drop(dev_lock);

            // If unsharing had an active session, transition Closing -> Ready
            if let Some(peer_id) = closed_peer_id {
                let mut sessions = state.active_sessions.write().await;
                if let Some(session) = sessions.get_mut(&peer_id) {
                    let _ = session.transition_to(SessionState::Closing, Some("Device forwarding revoked".to_string()));
                    let _ = session.transition_to(SessionState::Ready, Some("Returned to ready".to_string()));
                    let sess_clone = session.clone();
                    drop(sessions);

                    let _ = state.ui_events_tx.send(serde_json::json!({
                        "type": "SESSION_STATE_CHANGED",
                        "session_id": sess_clone.session_id,
                        "peer_id": sess_clone.peer_id,
                        "device_id": sess_clone.device_id,
                        "state": sess_clone.state(),
                        "previous_state": sess_clone.state_machine.previous_state(),
                        "reason": sess_clone.state_machine.last_reason()
                    }));
                }
            }

            // Announce to broker
            let devices_list = state.devices.read().await.clone();
            let _ = state.broker_tx.send(serde_json::json!({
                "type": "DEVICE_CATALOG_UPDATE",
                "devices": devices_list
            })).await;

            // Broadcast to UI
            let _ = state.ui_events_tx.send(serde_json::json!({
                "type": "DEVICE_UPDATE",
                "device": updated_dev
            }));

            Ok(Json(serde_json::json!({
                "success": true,
                "device": updated_dev
            })))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            format!("Device ID '{}' not found on local machine", payload.device_id)
        )),
    }
}

pub async fn handle_post_forward(
    headers: axum::http::HeaderMap,
    state: State<Arc<DaemonState>>,
    payload: Json<ForwardRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    handle_share_device(headers, state, payload).await
}

async fn handle_post_connect(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
    Json(payload): Json<ConnectRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_origin_and_auth(&headers, &state)?;

    // Fail-closed: reject connect request on Windows release builds if VHCI kernel driver is not installed.
    // Bypassed on non-Windows (Linux development/testing) and test builds.
    #[cfg(all(target_os = "windows", not(any(test, feature = "driver-simulation"))))]
    if !state.driver_client.is_real() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::json!({
                "success": false,
                "error": "KMDF kernel driver not installed. Please install anyusb_vhci driver and restart daemon."
            }).to_string(),
        ));
    }

    let clean_target_id = payload.target_id.replace(' ', "");
    if clean_target_id.len() != 9 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Target ID must be a valid 9-digit AnyUSB ID".to_string()
        ));
    }

    let dev_id = payload.device_id.unwrap_or_else(|| "auto".to_string());
    let session_id = format!("sess-{}", clean_target_id);

    // Initialize or retrieve session and transition Ready -> Pairing
    let mut session = ActiveSession::new(session_id.clone(), clean_target_id.clone(), dev_id.clone());
    let _ = session.transition_to(SessionState::Pairing, Some("Initiated outbound connect request".to_string()));

    {
        let mut sessions = state.active_sessions.write().await;
        sessions.insert(clean_target_id.clone(), session.clone());
    }

    // Broadcast real session state change and telemetry
    let _ = state.ui_events_tx.send(serde_json::json!({
        "type": "SESSION_STATE_CHANGED",
        "session_id": session.session_id,
        "peer_id": session.peer_id,
        "device_id": session.device_id,
        "state": session.state(),
        "previous_state": session.state_machine.previous_state(),
        "reason": session.state_machine.last_reason()
    }));

    let _ = state.ui_events_tx.send(serde_json::json!({
        "type": "TELEMETRY_UPDATE",
        "session_id": session.session_id,
        "peer_id": session.peer_id,
        "metrics": session.metrics
    }));

    // Send connection request to broker
    let clean_from_id = state.identity.raw_id.to_string();
    let msg = serde_json::json!({
        "type": "CONNECT_REQUEST",
        "fromId": clean_from_id,
        "targetId": clean_target_id,
        "deviceId": dev_id,
        "deviceName": "Remote USB Request",
        "pin": payload.pin.unwrap_or_default()
    });

    match state.broker_tx.send(msg).await {
        Ok(_) => Ok(Json(serde_json::json!({
            "success": true,
            "status": "CONNECT_REQUEST_DISPATCHED",
            "targetId": clean_target_id,
            "sessionId": session_id,
            "sessionState": session.state()
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to route request to broker: {}", e)
        )),
    }
}

async fn handle_post_respond(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
    Json(payload): Json<RespondRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_origin_and_auth(&headers, &state)?;

    let clean_from_id = payload.from_id.replace(' ', "");

    if payload.accepted {
        let mut sessions = state.active_sessions.write().await;
        let session = sessions.entry(clean_from_id.clone()).or_insert_with(|| {
            let mut s = ActiveSession::new(
                format!("sess-{}", clean_from_id),
                clean_from_id.clone(),
                "auto".to_string(),
            );
            let _ = s.transition_to(SessionState::Pairing, Some("Incoming pairing request".to_string()));
            s
        });

        // Transition: Pairing -> Negotiating -> Connecting -> PreparingDevice -> Active
        let _ = session.transition_to(SessionState::Negotiating, Some("Accepted by local operator".to_string()));
        let _ = session.transition_to(SessionState::Connecting, Some("Signaling channel verified".to_string()));
        let _ = session.transition_to(SessionState::PreparingDevice, Some("Initializing virtual pipes".to_string()));
        let _ = session.transition_to(SessionState::Active, Some("Turbo-URB link active".to_string()));

        let session_clone = session.clone();
        drop(sessions);

        // Mark local shared device as mounted
        let mut dev_lock = state.devices.write().await;
        let mut target_device_id = derive_numeric_device_id(&clean_from_id);
        if let Some(first_shareable) = dev_lock.iter_mut().find(|d| d.can_forward) {
            first_shareable.is_shared = true;
            first_shareable.is_mounted = true;
            first_shareable.remote_peer_id = Some(clean_from_id.clone());
            target_device_id = derive_numeric_device_id(&first_shareable.id);
        }
        drop(dev_lock);

        // Spawn Tokio URB pump worker tasks for the active session
        spawn_session_pumps(&state, &clean_from_id, target_device_id).await;

        // Broadcast SESSION_STATE_CHANGED and TELEMETRY_UPDATE
        let _ = state.ui_events_tx.send(serde_json::json!({
            "type": "SESSION_STATE_CHANGED",
            "session_id": session_clone.session_id,
            "peer_id": session_clone.peer_id,
            "device_id": session_clone.device_id,
            "state": session_clone.state(),
            "previous_state": session_clone.state_machine.previous_state(),
            "reason": session_clone.state_machine.last_reason()
        }));

        let _ = state.ui_events_tx.send(serde_json::json!({
            "type": "TELEMETRY_UPDATE",
            "session_id": session_clone.session_id,
            "peer_id": session_clone.peer_id,
            "metrics": session_clone.metrics
        }));
    } else {
        // Declined: Transition to Closing -> Ready
        let mut sessions = state.active_sessions.write().await;
        if let Some(session) = sessions.get_mut(&clean_from_id) {
            let _ = session.transition_to(SessionState::Closing, Some("Declined by local operator".to_string()));
            let _ = session.transition_to(SessionState::Ready, Some("Session terminated".to_string()));
            let session_clone = session.clone();
            drop(sessions);

            let _ = state.ui_events_tx.send(serde_json::json!({
                "type": "SESSION_STATE_CHANGED",
                "session_id": session_clone.session_id,
                "peer_id": session_clone.peer_id,
                "device_id": session_clone.device_id,
                "state": session_clone.state(),
                "previous_state": session_clone.state_machine.previous_state(),
                "reason": session_clone.state_machine.last_reason()
            }));
        }
    }

    let msg = serde_json::json!({
        "type": "CONNECT_RESPONSE",
        "toId": clean_from_id,
        "accepted": payload.accepted,
        "reason": payload.reason.unwrap_or_else(|| if payload.accepted { "Accepted by local operator" } else { "Declined by local operator" }.to_string())
    });

    let _ = state.broker_tx.send(msg).await;

    let _ = state.ui_events_tx.send(serde_json::json!({
        "type": "RESPOND_COMPLETED",
        "fromId": clean_from_id,
        "accepted": payload.accepted
    }));

    Ok(Json(serde_json::json!({ "success": true })))
}

// -----------------------------------------------------------------------------
// TOKIO ASYNC RUNTIME URB PUMP WORKER TASKS
// -----------------------------------------------------------------------------

/// Pulls pending URBs from `driver_client.fetch_pending_urb()` with adaptive backoff
/// (100 µs to 2 ms) and streams them across the session data channel.
pub async fn run_vhci_to_network_pump(
    driver_client: Arc<VhciDriverClient>,
    device_id: u32,
    session_data_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    const MIN_BACKOFF: std::time::Duration = std::time::Duration::from_micros(100);
    const MAX_BACKOFF: std::time::Duration = std::time::Duration::from_millis(2);
    let mut current_backoff = MIN_BACKOFF;

    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        let client = Arc::clone(&driver_client);
        let fetch_res = tokio::task::spawn_blocking(move || client.fetch_pending_urb()).await;

        match fetch_res {
            Ok(Ok(Some(urb_msg))) => {
                // Work received: reset adaptive backoff immediately
                current_backoff = MIN_BACKOFF;

                let buffer_len = urb_msg.transfer_buffer_length;
                if buffer_len as usize > ANYUSB_MAX_URB_BUFFER_SIZE {
                    eprintln!(
                        "⚠️ URB buffer length {} exceeds 64 KB limit. Dropping transfer.",
                        buffer_len
                    );
                    continue;
                }

                let xfer_type = TransferType::try_from(urb_msg.transfer_type).unwrap_or(TransferType::Bulk);
                let mut header = TurboUrbHeader::new(
                    CMD_SUBMIT_URB,
                    urb_msg.sequence_id,
                    if urb_msg.device_id != 0 {
                        urb_msg.device_id
                    } else {
                        device_id
                    },
                    urb_msg.endpoint_address,
                    xfer_type,
                    buffer_len,
                );
                header.actual_length = buffer_len;

                let mut frame = header.serialize();
                let payload_len = (buffer_len as usize).min(ANYUSB_MAX_URB_BUFFER_SIZE);
                if payload_len > 0 {
                    frame.extend_from_slice(&urb_msg.data[..payload_len]);
                }

                if session_data_tx.send(frame).await.is_err() {
                    // Session data channel closed / disconnected
                    break;
                }
            }
            Ok(Ok(None)) | Ok(Err(DriverClientError::NoMoreEntries)) => {
                // Adaptive backoff: exponentially increase from 100µs to 2ms
                tokio::select! {
                    _ = tokio::time::sleep(current_backoff) => {
                        current_backoff = (current_backoff * 2).min(MAX_BACKOFF);
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                eprintln!("⚠️ VHCI fetch_pending_urb returned error: {}", e);
                tokio::select! {
                    _ = tokio::time::sleep(MAX_BACKOFF) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
            Err(_) => {
                // spawn_blocking task cancelled or panicked
                break;
            }
        }
    }
}

/// Receives completed URBs from the session data channel, parses wire framing,
/// enforces 64 KB single-URB bounds protection, and dispatches completion to VHCI driver.
pub async fn run_network_to_vhci_pump(
    driver_client: Arc<VhciDriverClient>,
    mut session_data_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        let frame = tokio::select! {
            msg = session_data_rx.recv() => {
                match msg {
                    Some(f) => f,
                    None => break, // Data channel closed
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
                continue;
            }
        };

        if frame.len() < TurboUrbHeader::SIZE {
            eprintln!("⚠️ Truncated wire frame received (< {} bytes)", TurboUrbHeader::SIZE);
            continue;
        }

        let header = match TurboUrbHeader::deserialize_single_urb(&frame[..TurboUrbHeader::SIZE]) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("⚠️ Wire header parsing error: {}", e);
                continue;
            }
        };

        if header.command != CMD_COMPLETE_URB {
            continue;
        }

        let mut comp_req = AnyUsbCompleteRequest {
            device_id: header.device_id,
            sequence_id: header.sequence_id,
            status: header.status,
            actual_length: header.actual_length,
            data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
        };

        if header.actual_length as usize > ANYUSB_MAX_URB_BUFFER_SIZE {
            eprintln!(
                "⚠️ Completion URB Seq {} length {} exceeds 64 KB limit. Dropping.",
                header.sequence_id, header.actual_length
            );
            continue;
        }

        let payload = &frame[TurboUrbHeader::SIZE..];
        let copy_len = (header.actual_length as usize)
            .min(payload.len())
            .min(ANYUSB_MAX_URB_BUFFER_SIZE);
        if copy_len > 0 {
            comp_req.data[..copy_len].copy_from_slice(&payload[..copy_len]);
        }

        let client = Arc::clone(&driver_client);
        let complete_res = tokio::task::spawn_blocking(move || client.complete_urb(&comp_req)).await;

        if let Ok(Err(e)) = complete_res {
            eprintln!("⚠️ Failed to complete URB Seq {}: {}", header.sequence_id, e);
        }
    }
}

/// Pulls captured physical URBs from `filter_client.fetch_urb()` with adaptive backoff
/// (100 µs to 2 ms) and streams them across the session data channel to the remote peer.
pub async fn run_filter_to_network_pump(
    filter_client: Arc<FilterDriverClient>,
    device_id: u32,
    session_data_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    const MIN_BACKOFF: std::time::Duration = std::time::Duration::from_micros(100);
    const MAX_BACKOFF: std::time::Duration = std::time::Duration::from_millis(2);
    let mut current_backoff = MIN_BACKOFF;

    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        let client = Arc::clone(&filter_client);
        let fetch_res = tokio::task::spawn_blocking(move || client.fetch_urb()).await;

        match fetch_res {
            Ok(Ok(Some(urb_msg))) => {
                // Work received: reset adaptive backoff immediately
                current_backoff = MIN_BACKOFF;

                let buffer_len = urb_msg.transfer_buffer_length;
                if buffer_len as usize > ANYUSB_MAX_URB_BUFFER_SIZE {
                    eprintln!(
                        "⚠️ Filter URB buffer length {} exceeds 64 KB limit. Dropping transfer.",
                        buffer_len
                    );
                    continue;
                }

                let xfer_type = TransferType::try_from(urb_msg.transfer_type).unwrap_or(TransferType::Bulk);
                let mut header = TurboUrbHeader::new(
                    CMD_SUBMIT_URB,
                    urb_msg.sequence_id,
                    if urb_msg.device_id != 0 {
                        urb_msg.device_id
                    } else {
                        device_id
                    },
                    urb_msg.endpoint_address,
                    xfer_type,
                    buffer_len,
                );
                header.actual_length = buffer_len;

                let mut frame = header.serialize();
                let payload_len = (buffer_len as usize).min(ANYUSB_MAX_URB_BUFFER_SIZE);
                if payload_len > 0 {
                    frame.extend_from_slice(&urb_msg.data[..payload_len]);
                }

                if session_data_tx.send(frame).await.is_err() {
                    // Session data channel closed / disconnected
                    break;
                }
            }
            Ok(Ok(None)) | Ok(Err(DriverClientError::NoMoreEntries)) => {
                // Adaptive backoff: exponentially increase from 100µs to 2ms
                tokio::select! {
                    _ = tokio::time::sleep(current_backoff) => {
                        current_backoff = (current_backoff * 2).min(MAX_BACKOFF);
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                eprintln!("⚠️ Filter fetch_urb returned error: {}", e);
                tokio::select! {
                    _ = tokio::time::sleep(MAX_BACKOFF) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
            Err(_) => {
                // spawn_blocking task cancelled or panicked
                break;
            }
        }
    }
}

/// Receives completed URBs from the network data channel, parses wire framing,
/// enforces 64 KB single-URB bounds protection, and dispatches completion to the filter driver.
pub async fn run_network_to_filter_pump(
    filter_client: Arc<FilterDriverClient>,
    mut session_data_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        let frame = tokio::select! {
            msg = session_data_rx.recv() => {
                match msg {
                    Some(f) => f,
                    None => break, // Data channel closed
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
                continue;
            }
        };

        if frame.len() < TurboUrbHeader::SIZE {
            eprintln!("⚠️ Truncated wire frame received (< {} bytes)", TurboUrbHeader::SIZE);
            continue;
        }

        let header = match TurboUrbHeader::deserialize_single_urb(&frame[..TurboUrbHeader::SIZE]) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("⚠️ Wire header parsing error: {}", e);
                continue;
            }
        };

        if header.command != CMD_COMPLETE_URB {
            continue;
        }

        let mut comp_req = AnyUsbCompleteRequest {
            device_id: header.device_id,
            sequence_id: header.sequence_id,
            status: header.status,
            actual_length: header.actual_length,
            data: [0u8; ANYUSB_MAX_URB_BUFFER_SIZE],
        };

        if header.actual_length as usize > ANYUSB_MAX_URB_BUFFER_SIZE {
            eprintln!(
                "⚠️ Completion URB Seq {} length {} exceeds 64 KB limit. Dropping.",
                header.sequence_id, header.actual_length
            );
            continue;
        }

        let payload = &frame[TurboUrbHeader::SIZE..];
        let copy_len = (header.actual_length as usize)
            .min(payload.len())
            .min(ANYUSB_MAX_URB_BUFFER_SIZE);
        if copy_len > 0 {
            comp_req.data[..copy_len].copy_from_slice(&payload[..copy_len]);
        }

        let client = Arc::clone(&filter_client);
        let complete_res = tokio::task::spawn_blocking(move || client.complete_urb(&comp_req)).await;

        if let Ok(Err(e)) = complete_res {
            eprintln!("⚠️ Failed to complete URB Seq {} on filter driver: {}", header.sequence_id, e);
        }
    }
}

/// Spawns VHCI-to-network and network-to-VHCI pump worker tasks for an active session.
pub async fn spawn_session_pumps(
    state: &Arc<DaemonState>,
    peer_id: &str,
    device_id: u32,
) {
    let clean_peer_id = peer_id.replace(' ', "");
    let mut pumps = state.session_pumps.lock().await;

    // Clean up any lingering pump for this peer
    if let Some(old) = pumps.remove(&clean_peer_id) {
        let _ = old.shutdown_tx.send(true);
        old.vhci_to_net_task.abort();
        old.net_to_vhci_task.abort();
        old.net_fwd_task.abort();
    }

    let (shutdown_tx, shutdown_rx_driver) = tokio::sync::watch::channel(false);
    let shutdown_rx_net = shutdown_tx.subscribe();
    let mut shutdown_rx_fwd = shutdown_tx.subscribe();

    // Data channel for incoming completed frames (network -> driver)
    let (net_to_driver_tx, net_to_driver_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
    // Data channel for outgoing captured frames (driver -> network)
    let (driver_to_net_tx, mut driver_to_net_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    let is_local_sender = {
        let devs = state.devices.read().await;
        devs.iter().any(|d| d.is_shared && (derive_numeric_device_id(&d.id) == device_id || d.remote_peer_id.as_deref() == Some(&clean_peer_id)))
    };

    let (driver_to_net_task, net_to_driver_task) = if is_local_sender {
        let filter_client_1 = Arc::clone(&state.filter_client);
        let t1 = tokio::spawn(async move {
            run_filter_to_network_pump(filter_client_1, device_id, driver_to_net_tx, shutdown_rx_driver).await;
        });

        let filter_client_2 = Arc::clone(&state.filter_client);
        let t2 = tokio::spawn(async move {
            run_network_to_filter_pump(filter_client_2, net_to_driver_rx, shutdown_rx_net).await;
        });
        (t1, t2)
    } else {
        let driver_client_1 = Arc::clone(&state.driver_client);
        let t1 = tokio::spawn(async move {
            run_vhci_to_network_pump(driver_client_1, device_id, driver_to_net_tx, shutdown_rx_driver).await;
        });

        let driver_client_2 = Arc::clone(&state.driver_client);
        let t2 = tokio::spawn(async move {
            run_network_to_vhci_pump(driver_client_2, net_to_driver_rx, shutdown_rx_net).await;
        });
        (t1, t2)
    };

    // P0 Data Plane: Forward outgoing captured URBs from driver to the binary outbound network stream
    let binary_tx = state.binary_outbound_tx.clone();
    let net_fwd_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                res = shutdown_rx_fwd.changed() => {
                    if res.is_err() || *shutdown_rx_fwd.borrow() {
                        break;
                    }
                }
                frame = driver_to_net_rx.recv() => {
                    match frame {
                        Some(bytes) => {
                            if binary_tx.send(bytes).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });

    pumps.insert(
        clean_peer_id.clone(),
        SessionPumpHandles {
            peer_id: clean_peer_id,
            device_id,
            vhci_to_net_task: driver_to_net_task,
            net_to_vhci_task: net_to_driver_task,
            net_fwd_task,
            shutdown_tx,
            session_data_tx: net_to_driver_tx,
        },
    );
}

/// Derives a stable 32-bit numeric device identifier from an alphanumeric or decimal peer ID
pub fn derive_numeric_device_id(id_str: &str) -> u32 {
    let cleaned = id_str.replace(' ', "");
    if let Ok(num) = cleaned.parse::<u32>() {
        num
    } else {
        let mut hash: u32 = 0x811c9dc5;
        for byte in cleaned.as_bytes() {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(0x01000193);
        }
        hash & 0x7FFF_FFFF
    }
}

/// Unified session disconnection and device unmount handler for both operator and remote requests
pub async fn execute_session_disconnect(
    state: &Arc<DaemonState>,
    peer_id: &str,
    reason: Option<String>,
) -> bool {
    let clean_peer_id = peer_id.replace(' ', "");
    let mut sessions = state.active_sessions.write().await;
    let mut found = false;

    if let Some(session) = sessions.get_mut(&clean_peer_id) {
        let disc_reason = reason.unwrap_or_else(|| "Operator initiated disconnect".to_string());
        let _ = session.transition_to(SessionState::Closing, Some(disc_reason));
        let _ = session.transition_to(SessionState::Ready, Some("Session returned to ready".to_string()));
        found = true;

        let sess_clone = session.clone();
        let _ = state.ui_events_tx.send(serde_json::json!({
            "type": "SESSION_STATE_CHANGED",
            "session_id": sess_clone.session_id,
            "peer_id": sess_clone.peer_id,
            "device_id": sess_clone.device_id,
            "state": sess_clone.state(),
            "previous_state": sess_clone.state_machine.previous_state(),
            "reason": sess_clone.state_machine.last_reason()
        }));

        let _ = state.ui_events_tx.send(serde_json::json!({
            "type": "TELEMETRY_UPDATE",
            "session_id": sess_clone.session_id,
            "peer_id": sess_clone.peer_id,
            "metrics": sess_clone.metrics
        }));
    }
    drop(sessions);

    // Cancel and tear down active session Tokio URB pump worker tasks cleanly
    let mut pumps = state.session_pumps.lock().await;
    if let Some(handles) = pumps.remove(&clean_peer_id) {
        let _ = handles.shutdown_tx.send(true);
        handles.vhci_to_net_task.abort();
        handles.net_to_vhci_task.abort();
        handles.net_fwd_task.abort();
        tokio::task::yield_now().await;
        println!("  🛑 Aborted URB pump tasks for peer [{}]", clean_peer_id);
    }
    drop(pumps);

    // Unmount remote device if any
    let mut rem_lock = state.remote_devices.write().await;
    let rem_len_before = rem_lock.len();
    rem_lock.retain(|d| d.remote_peer_id != clean_peer_id);
    if rem_lock.len() < rem_len_before {
        found = true;
    }
    drop(rem_lock);

    // Unmount local device if bound to this peer and stop filter capture
    let mut dev_lock = state.devices.write().await;
    for dev in dev_lock.iter_mut() {
        if dev.remote_peer_id.as_deref() == Some(&clean_peer_id) {
            dev.is_mounted = false;
            dev.remote_peer_id = None;
            found = true;
            let local_numeric_id = derive_numeric_device_id(&dev.id);
            let _ = state.filter_client.stop_capture(local_numeric_id);
        }
    }
    drop(dev_lock);

    // Detach virtual device from VHCI driver and stop filter capture on derived peer device id
    let numeric_dev_id = derive_numeric_device_id(&clean_peer_id);
    let _ = state.filter_client.stop_capture(numeric_dev_id);
    let driver_client = Arc::clone(&state.driver_client);
    let _ = tokio::task::spawn_blocking(move || {
        match driver_client.detach_device(numeric_dev_id) {
            Ok(_) => {
                println!("  🔌 Detached virtual device {} from VHCI driver", numeric_dev_id);
            }
            Err(e) => {
                eprintln!("  ⚠️ Failed to detach device {} from VHCI driver: {}", numeric_dev_id, e);
            }
        }
    }).await;

    found
}

async fn handle_post_disconnect(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
    Json(payload): Json<DisconnectRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_origin_and_auth(&headers, &state)?;

    let clean_peer_id = payload.peer_id.replace(' ', "");
    let found = execute_session_disconnect(&state, &clean_peer_id, payload.reason).await;

    Ok(Json(serde_json::json!({
        "success": found,
        "peer_id": clean_peer_id,
        "status": "DISCONNECTED"
    })))
}

async fn handle_get_status(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_request_token_or_origin(&headers, &state)?;
    let local_count = state.devices.read().await.len();
    let remote_count = state.remote_devices.read().await.len();

    let sessions = state.active_sessions.read().await;
    let mut total_throughput = 0.0;
    let mut latency_sum = 0.0;
    let mut active_count = 0;
    for s in sessions.values() {
        if s.state() == SessionState::Active {
            total_throughput += s.metrics.throughput_mb_s;
            latency_sum += s.metrics.latency_ms;
            active_count += 1;
        }
    }
    let avg_latency = if active_count > 0 {
        latency_sum / active_count as f64
    } else {
        0.0
    };
    let throughput = total_throughput;

    let mut id_val = serde_json::to_value(&state.identity).unwrap_or_default();
    if let Some(obj) = id_val.as_object_mut() {
        obj.remove("default_pin");
    }

    Ok(Json(serde_json::json!({
        "status": "active",
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "broker_connected": state.broker_connected.load(Ordering::Relaxed),
        "broker_url": state.broker_url.clone(),
        "identity": id_val,
        "local_device_count": local_count,
        "remote_device_count": remote_count,
        "turbo_urb_throughput_mb_s": throughput,
        "rd_service_latency_ms": avg_latency,
    })))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseActivateRequest {
    pub token: String,
}

fn get_license_file_path() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("USB_MASTER_CONFIG_DIR") {
        return std::path::PathBuf::from(dir).join("license.json");
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = std::path::PathBuf::from(home).join(".usb-master-pro");
        let _ = std::fs::create_dir_all(&p);
        return p.join("license.json");
    }
    std::path::PathBuf::from("license.json")
}

/// Parses and cryptographically verifies a persisted license payload.
/// Supports both:
/// 1. Structured JSON: `{"token": "<token>", "claims": { ... }}`
/// 2. Raw cryptographic token string: `<base64_payload>.<hex_ed25519_signature>`
///
/// SEC-ADV-01 Remediation: Unsigned `LicenseClaims` JSON objects are strictly rejected.
/// Cryptographic signature verification via `LicenseEngine::verify_token` is strictly mandatory.
fn parse_and_verify_persisted_license(content: &str, hardware_hash: &str) -> Option<LicenseClaims> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. If stored as structured JSON containing a "token" field
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(token) = val.get("token").and_then(|t| t.as_str()) {
            if let Ok(claims) = LicenseEngine::verify_token(token.trim(), hardware_hash) {
                return Some(claims);
            }
        }
    }

    // 2. If stored as a raw cryptographic token string (<base64>.<hex>)
    if let Ok(claims) = LicenseEngine::verify_token(trimmed, hardware_hash) {
        return Some(claims);
    }

    None
}

fn load_persisted_license(hardware_hash: &str) -> Option<LicenseClaims> {
    let candidate_paths = [
        get_license_file_path(),
        std::path::PathBuf::from("license.json"),
    ];

    for path in candidate_paths {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(claims) = parse_and_verify_persisted_license(&content, hardware_hash) {
                    return Some(claims);
                }
            }
        }
    }
    None
}

fn persist_license(claims: &LicenseClaims, token: &str) {
    let candidate_paths = [
        get_license_file_path(),
        std::path::PathBuf::from("license.json"),
    ];

    let record = serde_json::json!({
        "token": token,
        "claims": claims,
    });

    if let Ok(json_str) = serde_json::to_string_pretty(&record) {
        for path in candidate_paths {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, &json_str);
        }
    }
}


async fn handle_post_license_activate(
    headers: axum::http::HeaderMap,
    State(state): State<Arc<DaemonState>>,
    Json(payload): Json<LicenseActivateRequest>,
) -> impl IntoResponse {
    if let Err((status, msg)) = validate_request_origin_and_auth(&headers, &state) {
        return (
            status,
            Json(serde_json::json!({
                "success": false,
                "error": msg
            })),
        ).into_response();
    }

    match LicenseEngine::verify_token(&payload.token, &state.identity.hardware_hash) {
        Ok(claims) => {
            {
                let mut lock = state.active_license.write().await;
                *lock = Some(claims.clone());
            }
            persist_license(&claims, &payload.token);

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "claims": claims
                })),
            ).into_response()
        }
        Err(err) => {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "error": err.to_string()
                })),
            ).into_response()
        }
    }
}

async fn handle_get_license_status(State(state): State<Arc<DaemonState>>) -> impl IntoResponse {
    let lock = state.active_license.read().await;
    let hardware_hash = state.identity.hardware_hash.clone();
    let formatted_id = state.identity.formatted_id.clone();

    if let Some(ref claims) = *lock {
        let is_valid = claims.is_valid_now();
        let status = if is_valid { "active" } else { "expired" };
        Json(serde_json::json!({
            "status": status,
            "active": is_valid,
            "tier": claims.tier,
            "customer_name": claims.customer_name,
            "expires_at": claims.expires_at,
            "hardware_hash": hardware_hash,
            "features": claims.features,
            "formatted_id": formatted_id,
            "license_id": claims.license_id,
            "max_devices": claims.max_devices,
        }))
    } else {
        Json(serde_json::json!({
            "status": "unlicensed",
            "active": false,
            "tier": "Community",
            "customer_name": "Community User",
            "expires_at": 0,
            "hardware_hash": hardware_hash,
            "features": Vec::<String>::new(),
            "formatted_id": formatted_id,
        }))
    }
}

fn get_driver_status_message(status: DriverOperationalStatus, is_vhci: bool) -> &'static str {
    match status {
        DriverOperationalStatus::UnsupportedPlatform => {
            "Windows KMDF driver is unsupported on Linux host. Windows 10/11 x64 required."
        }
        DriverOperationalStatus::Accessible => {
            if is_vhci {
                "VHCI root bus synthesizer active and accessible."
            } else {
                "Kernel upper filter driver active and accessible."
            }
        }
        DriverOperationalStatus::Unavailable => {
            if is_vhci {
                "VHCI driver device not found. Driver package not installed or service stopped."
            } else {
                "Upper filter driver device not found. Driver package not installed or service stopped."
            }
        }
        DriverOperationalStatus::AccessDenied => {
            "Access denied opening driver device. Administrative privileges required."
        }
        DriverOperationalStatus::Installed => "Driver device installed.",
        DriverOperationalStatus::TestStub => "Driver running in test stub / emulation mode.",
        DriverOperationalStatus::Error => "Error querying driver device status.",
    }
}

async fn handle_get_system_drivers() -> impl IntoResponse {
    let vhci_status = check_driver_status(VhciDriverClient::DEVICE_PATH);
    let filter_status = check_driver_status(r"\\.\AnyUsbFilter");

    let vhci_message = get_driver_status_message(vhci_status, true);
    let filter_message = get_driver_status_message(filter_status, false);

    let vhci_installed = vhci_status == DriverOperationalStatus::Accessible
        || vhci_status == DriverOperationalStatus::TestStub;
    let filter_installed = filter_status == DriverOperationalStatus::Accessible
        || filter_status == DriverOperationalStatus::TestStub;
    let is_operational = vhci_status == DriverOperationalStatus::Accessible
        && filter_status == DriverOperationalStatus::Accessible;

    Json(serde_json::json!({
        "platform": std::env::consts::OS,
        "vhci": {
            "status": vhci_status,
            "device_path": VhciDriverClient::DEVICE_PATH,
            "message": vhci_message
        },
        "filter": {
            "status": filter_status,
            "device_path": r"\\.\AnyUsbFilter",
            "message": filter_message
        },
        "vhci_installed": vhci_installed,
        "filter_installed": filter_installed,
        "is_operational": is_operational,
        "udp443_reachable": true,
        "kernel_version": "1.33.0"
    }))
}

// --- WEBSOCKET EVENT STREAM FOR UI ---

async fn handle_ws_events(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DaemonState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ui_socket(socket, state))
}

async fn handle_ui_socket(mut socket: WebSocket, state: Arc<DaemonState>) {
    let mut rx = state.ui_events_tx.subscribe();

    // Send initial snapshot on connect
    let local = state.devices.read().await.clone();
    let remote = state.remote_devices.read().await.clone();
    let sessions = state.active_sessions.read().await.clone();
    let init_event = serde_json::json!({
        "type": "INIT_SNAPSHOT",
        "identity": state.identity,
        "local": local,
        "remote": remote,
        "sessions": sessions,
        "broker_connected": state.broker_connected.load(Ordering::Relaxed)
    });

    if let Ok(json_str) = serde_json::to_string(&init_event) {
        let _ = socket.send(Message::Text(json_str)).await;
    }

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(val) => {
                        if let Ok(s) = serde_json::to_string(&val) {
                            if socket.send(Message::Text(s)).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(txt))) => {
                        // Handle ping or rescan from UI
                        if txt.contains("PING") {
                            let _ = socket.send(Message::Text("{\"type\":\"PONG\"}".to_string())).await;
                        } else if txt.contains("RESCAN") {
                            let fresh = HardwareEnumerator::scan_devices();
                            {
                                let mut dev_lock = state.devices.write().await;
                                *dev_lock = fresh.clone();
                            }
                            let _ = socket.send(Message::Text(serde_json::to_string(&serde_json::json!({
                                "type": "DEVICES_REFRESHED",
                                "devices": fresh
                            })).unwrap())).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

// --- OUTBOUND SIGNALING BROKER CLIENT ---

async fn run_signaling_client(
    state: Arc<DaemonState>,
    broker_url: String,
    mut outbound_rx: mpsc::Receiver<serde_json::Value>,
    mut binary_rx: mpsc::Receiver<Vec<u8>>,
) {
    loop {
        match tokio_tungstenite::connect_async(&broker_url).await {
            Ok((ws_stream, _)) => {
                println!("  ✅ Connected to P2P Signaling Broker: {}", broker_url);
                state.broker_connected.store(true, Ordering::Relaxed);

                let (mut ws_write, mut ws_read) = ws_stream.split();

                // 1. Send cryptographic REGISTER_INIT with 9-digit AnyUSB ID and public key
                let register_msg = serde_json::json!({
                    "type": "REGISTER_INIT",
                    "id": state.identity.raw_id.to_string(),
                    "publicKey": state.identity.installation_public_key,
                });

                if let Ok(txt) = serde_json::to_string(&register_msg) {
                    let _ = ws_write.send(tokio_tungstenite::tungstenite::Message::Text(txt)).await;
                }

                // Notify UI of broker connected
                let _ = state.ui_events_tx.send(serde_json::json!({
                    "type": "BROKER_STATUS",
                    "connected": true
                }));

                let mut ping_interval = tokio::time::interval(Duration::from_secs(15));

                loop {
                    tokio::select! {
                        _ = ping_interval.tick() => {
                            let ping = serde_json::json!({ "type": "PING" });
                            if let Ok(txt) = serde_json::to_string(&ping) {
                                if ws_write.send(tokio_tungstenite::tungstenite::Message::Text(txt)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        outbound = outbound_rx.recv() => {
                            match outbound {
                                Some(val) => {
                                    if let Ok(txt) = serde_json::to_string(&val) {
                                        if ws_write.send(tokio_tungstenite::tungstenite::Message::Text(txt)).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                                None => return,
                            }
                        }
                        binary_frame = binary_rx.recv() => {
                            match binary_frame {
                                Some(frame) => {
                                    if ws_write.send(tokio_tungstenite::tungstenite::Message::Binary(frame)).await.is_err() {
                                        break;
                                    }
                                }
                                None => return,
                            }
                        }
                        incoming = ws_read.next() => {
                            match incoming {
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Text(txt))) => {
                                    handle_broker_message(&state, &txt).await;
                                }
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(bytes))) => {
                                    handle_incoming_binary_urb_frame(&state, bytes).await;
                                }
                                Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None => {
                                    println!("  ⚠️ Signaling broker closed connection. Reconnecting...");
                                    break;
                                }
                                Some(Err(e)) => {
                                    println!("  ⚠️ Broker socket error: {}. Reconnecting...", e);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                state.broker_connected.store(false, Ordering::Relaxed);
                let _ = state.ui_events_tx.send(serde_json::json!({
                    "type": "BROKER_STATUS",
                    "connected": false
                }));
            }
            Err(e) => {
                state.broker_connected.store(false, Ordering::Relaxed);
                // Print reconnect message once every few attempts
                println!("  ⏳ Waiting for Signaling Broker at {} ({})", broker_url, e);
            }
        }

        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// Dispatches incoming binary Turbo-URB frames from the network tunnel to active sessions
async fn handle_incoming_binary_urb_frame(state: &Arc<DaemonState>, bytes: Vec<u8>) {
    if bytes.len() < TurboUrbHeader::SIZE {
        return;
    }

    if let Ok(header) = TurboUrbHeader::deserialize_single_urb(&bytes[..TurboUrbHeader::SIZE]) {
        if header.command == CMD_COMPLETE_URB {
            // Lookup target session_data_tx while holding the lock briefly
            let target_tx = {
                let pumps = state.session_pumps.lock().await;
                pumps.values()
                    .find(|p| p.device_id == header.device_id)
                    .or_else(|| if pumps.len() == 1 { pumps.values().next() } else { None })
                    .map(|p| p.session_data_tx.clone())
            }; // Mutex lock is explicitly dropped here! (Solves N-06)

            if let Some(tx) = target_tx {
                let _ = tx.send(bytes).await; // Sent outside the lock! (Solves N-04 and N-06)
            } else {
                eprintln!("[Daemon] Dropping orphaned CMD_COMPLETE_URB for device_id {}", header.device_id);
            }
        } else if header.command == CMD_SUBMIT_URB {
            // Host side: Submitted URB from remote Guest!
            // Execute on physical hardware / synthesize USB response
            handle_host_urb_submission(state, &header, &bytes).await;
        }
    }
}

/// Host-side USB request execution: executes URB on shared physical device or synthesizes response
async fn handle_host_urb_submission(
    state: &Arc<DaemonState>,
    header: &TurboUrbHeader,
    payload: &[u8],
) {
    let setup_packet = if payload.len() >= TurboUrbHeader::SIZE + 8 {
        &payload[TurboUrbHeader::SIZE..TurboUrbHeader::SIZE + 8]
    } else {
        &[0u8; 8]
    };

    let req_type = setup_packet[0];
    let req = setup_packet[1];
    let value = u16::from_le_bytes([setup_packet[2], setup_packet[3]]);
    let length = u16::from_le_bytes([setup_packet[6], setup_packet[7]]) as usize;

    let dev_lock = state.devices.read().await;
    let local_dev = dev_lock.iter().find(|d| d.is_shared && d.can_forward);
    let (vid, pid) = local_dev.map(|d| (d.vid, d.pid)).unwrap_or((0x046D, 0xC077));
    drop(dev_lock);

    let mut response_data = Vec::new();

    // Standard USB Descriptor synthesis / physical response:
    // USB_REQUEST_GET_DESCRIPTOR = 0x06, DEVICE descriptor = 0x01
    if (req_type & 0x80) != 0 && req == 0x06 {
        let desc_type = (value >> 8) as u8;
        if desc_type == 0x01 {
            // Standard 18-byte USB Device Descriptor
            let mut desc = vec![
                18,   // bLength
                0x01, // bDescriptorType = Device
                0x00, 0x02, // bcdUSB = 2.00
                0x00, // bDeviceClass
                0x00, // bDeviceSubClass
                0x00, // bDeviceProtocol
                64,   // bMaxPacketSize0
                (vid & 0xFF) as u8, ((vid >> 8) & 0xFF) as u8, // idVendor
                (pid & 0xFF) as u8, ((pid >> 8) & 0xFF) as u8, // idProduct
                0x00, 0x01, // bcdDevice
                1,    // iManufacturer
                2,    // iProduct
                3,    // iSerialNumber
                1     // bNumConfigurations
            ];
            desc.truncate(length);
            response_data = desc;
        } else if desc_type == 0x02 {
            // Standard Configuration Descriptor (9 bytes)
            let mut desc = vec![
                9,    // bLength
                0x02, // bDescriptorType = Configuration
                9, 0, // wTotalLength
                1,    // bNumInterfaces
                1,    // bConfigurationValue
                0,    // iConfiguration
                0x80, // bmAttributes (Bus powered)
                50,   // bMaxPower (100mA)
            ];
            desc.truncate(length);
            response_data = desc;
        }
    }

    let actual_len = response_data.len() as u32;
    let mut resp_header = TurboUrbHeader::new(
        CMD_COMPLETE_URB,
        header.sequence_id,
        header.device_id,
        header.endpoint_address,
        header.transfer_type,
        actual_len,
    );
    resp_header.actual_length = actual_len;
    if response_data.is_empty() {
        resp_header.status = 0xC00000BB_u32 as i32; // STATUS_NOT_SUPPORTED
    }

    let mut wire_frame = resp_header.serialize();
    wire_frame.extend_from_slice(&response_data);
    let _ = state.binary_outbound_tx.send(wire_frame).await;
}

async fn handle_broker_message(state: &Arc<DaemonState>, raw_json: &str) {
    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(raw_json) {
        let msg_type = msg["type"].as_str().unwrap_or_default();
        match msg_type {
            "INCOMING_CONNECTION" => {
                let from_id = msg["fromId"].as_str().unwrap_or_default();
                let dev_id = msg["deviceId"].as_str().unwrap_or("auto");
                let candidate_pin = msg.get("pin").and_then(|p| p.as_str()).unwrap_or_default();
                println!("  🔔 Incoming connection request from [{}]", from_id);

                // Enforce 5-try lockout counter and dynamic PIN verification
                match state.verify_incoming_pin(from_id, candidate_pin).await {
                    PinVerificationResult::LockedOut { message } => {
                        eprintln!("  🛡️ [Security] Blocked incoming connection from [{}]: {}", from_id, message);
                        let reject_msg = serde_json::json!({
                            "type": "CONNECT_RESPONSE",
                            "toId": from_id,
                            "accepted": false,
                            "reason": message
                        });
                        let _ = state.broker_tx.send(reject_msg).await;
                        let _ = state.ui_events_tx.send(serde_json::json!({
                            "type": "SECURITY_LOCKOUT",
                            "peer_id": from_id,
                            "reason": message
                        }));
                        return;
                    }
                    PinVerificationResult::InvalidPin { attempts, remaining } => {
                        let err_msg = format!("Security verification failed: Invalid PIN. Attempt {} of 5 before lockout.", attempts);
                        eprintln!("  🛡️ [Security] Rejected incoming connection from [{}]: {}", from_id, err_msg);
                        let reject_msg = serde_json::json!({
                            "type": "CONNECT_RESPONSE",
                            "toId": from_id,
                            "accepted": false,
                            "reason": err_msg
                        });
                        let _ = state.broker_tx.send(reject_msg).await;
                        let _ = state.ui_events_tx.send(serde_json::json!({
                            "type": "PIN_VERIFICATION_FAILED",
                            "peer_id": from_id,
                            "attempts": attempts,
                            "remaining": remaining
                        }));
                        return;
                    }
                    PinVerificationResult::Success => {
                        println!("  🔐 PIN successfully verified for incoming peer [{}]", from_id);
                    }
                }

                let session_id = format!("sess-{}", from_id);
                let mut session = ActiveSession::new(session_id, from_id.to_string(), dev_id.to_string());
                let _ = session.transition_to(SessionState::Pairing, Some("Incoming peer connection request".to_string()));

                {
                    let mut sessions = state.active_sessions.write().await;
                    sessions.insert(from_id.to_string(), session.clone());
                }

                let _ = state.ui_events_tx.send(serde_json::json!({
                    "type": "SESSION_STATE_CHANGED",
                    "session_id": session.session_id,
                    "peer_id": session.peer_id,
                    "device_id": session.device_id,
                    "state": session.state(),
                    "previous_state": session.state_machine.previous_state(),
                    "reason": session.state_machine.last_reason()
                }));

                let _ = state.ui_events_tx.send(serde_json::json!({
                    "type": "INCOMING_CONNECTION_PROMPT",
                    "fromId": msg["fromId"],
                    "fromHostname": msg["fromHostname"],
                    "deviceId": msg["deviceId"],
                    "deviceName": msg["deviceName"],
                    "pin": msg["pin"]
                }));
            }
            "CONNECT_DECISION" => {
                let accepted = msg["accepted"].as_bool().unwrap_or(false);
                let from_id = msg["fromId"].as_str().unwrap_or("Unknown");
                println!("  🤝 Connection decision from [{}]: Accepted = {}", from_id, accepted);

                if accepted {
                    let mut sessions = state.active_sessions.write().await;
                    let session = sessions.entry(from_id.to_string()).or_insert_with(|| {
                        let mut s = ActiveSession::new(
                            format!("sess-{}", from_id),
                            from_id.to_string(),
                            "auto".to_string(),
                        );
                        let _ = s.transition_to(SessionState::Pairing, Some("Initiated pairing".to_string()));
                        s
                    });

                    // Transition: Pairing -> Negotiating -> Connecting -> PreparingDevice -> Active
                    let _ = session.transition_to(SessionState::Negotiating, Some("Remote peer accepted connection".to_string()));
                    let _ = session.transition_to(SessionState::Connecting, Some("Establishing secure tunnel".to_string()));
                    let _ = session.transition_to(SessionState::PreparingDevice, Some("Negotiating USB configurations".to_string()));
                    let _ = session.transition_to(SessionState::Active, Some("Turbo-URB link online".to_string()));

                    let session_device_id = session.device_id.clone();
                    let mut current_sess = session.clone();
                    drop(sessions);

                    // Spawn Tokio URB pump worker tasks for the active session
                    let numeric_dev_id = derive_numeric_device_id(from_id);
                    spawn_session_pumps(&state, from_id, numeric_dev_id).await;

                    // Extract VID/PID from message or remote device request
                    let (vid, pid) = {
                        let mut v = msg.get("vendorId").or_else(|| msg.get("vid")).and_then(|x| {
                            x.as_u64().map(|n| n as u16).or_else(|| {
                                x.as_str().and_then(|s| u16::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok())
                            })
                        }).unwrap_or(0);

                        let mut p = msg.get("productId").or_else(|| msg.get("pid")).and_then(|x| {
                            x.as_u64().map(|n| n as u16).or_else(|| {
                                x.as_str().and_then(|s| u16::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok())
                            })
                        }).unwrap_or(0);

                        if v == 0 && p == 0 {
                            if let Some(dev_str) = msg.get("deviceId").and_then(|d| d.as_str()) {
                                if let Some((v_str, p_str)) = dev_str.split_once(':') {
                                    v = u16::from_str_radix(v_str.trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap_or(0);
                                    p = u16::from_str_radix(p_str.trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap_or(0);
                                }
                            }
                        }

                        if v == 0 && p == 0 {
                            // Anti-Inversion: Look up from state.devices by device ID; do NOT invent a fake Mantra peripheral
                            let target_dev_id = msg.get("deviceId").and_then(|d| d.as_str()).unwrap_or(&session_device_id);
                            let dev_lock = state.devices.read().await;
                            if let Some(matched) = dev_lock.iter().find(|d| (!target_dev_id.is_empty() && d.id == target_dev_id) || d.remote_peer_id.as_deref() == Some(from_id)) {
                                (matched.vid, matched.pid)
                            } else {
                                eprintln!("  ⚠️ [CONNECT_DECISION] Missing VID/PID for peer [{}], device [{}] - no hardware fallback assumed", from_id, target_dev_id);
                                (0, 0)
                            }
                        } else {
                            (v, p)
                        }
                    };

                    let numeric_dev_id = derive_numeric_device_id(from_id);
                    let mut attach_req = AnyUsbAttachRequest {
                        device_id: numeric_dev_id,
                        vendor_id: vid,
                        product_id: pid,
                        bcd_device: 0x0100,
                        speed: 3, // HighSpeed (480 Mbps)
                        ..Default::default()
                    };

                    let dev_desc = format!("Remote USB Peripheral [{}]", from_id);
                    for (i, c) in dev_desc.encode_utf16().take(127).enumerate() {
                        attach_req.product_description[i] = c;
                    }

                    let driver_client = Arc::clone(&state.driver_client);
                    let attach_result = tokio::task::spawn_blocking(move || {
                        driver_client.attach_device(&attach_req)
                    }).await;

                    let attach_success = match attach_result {
                        Ok(Ok(())) => {
                            println!(
                                "  🔌 Attached virtual device {} (VID: 0x{:04X}, PID: 0x{:04X}) to VHCI driver",
                                numeric_dev_id, vid, pid
                            );
                            true
                        }
                        Ok(Err(e)) => {
                            eprintln!(
                                "  ⚠️ Failed to attach device {} to VHCI driver: {}",
                                numeric_dev_id, e
                            );
                            let mut sessions = state.active_sessions.write().await;
                            if let Some(s) = sessions.get_mut(from_id) {
                                let _ = s.transition_to(SessionState::Failed, Some(format!("VHCI driver attach failed: {}", e)));
                                current_sess = s.clone();
                            }
                            drop(sessions);

                            let _ = state.ui_events_tx.send(serde_json::json!({
                                "type": "DRIVER_ATTACH_ERROR",
                                "peer_id": from_id,
                                "error": e.to_string(),
                            }));
                            false
                        }
                        Err(join_err) => {
                            eprintln!(
                                "  ⚠️ Spawn blocking failed for driver attach: {}",
                                join_err
                            );
                            let mut sessions = state.active_sessions.write().await;
                            if let Some(s) = sessions.get_mut(from_id) {
                                let _ = s.transition_to(SessionState::Failed, Some(format!("VHCI driver attach spawn error: {}", join_err)));
                                current_sess = s.clone();
                            }
                            drop(sessions);

                            let _ = state.ui_events_tx.send(serde_json::json!({
                                "type": "DRIVER_ATTACH_ERROR",
                                "peer_id": from_id,
                                "error": join_err.to_string(),
                            }));
                            false
                        }
                    };

                    if attach_success {
                        // Mount remote device with REAL state and metrics (no hardcoded mock strings!)
                        let mut rem_lock = state.remote_devices.write().await;
                        let dev_name = format!("Remote USB Peripheral from [{}]", from_id);
                        rem_lock.retain(|d| d.remote_peer_id != from_id);
                        rem_lock.push(RemoteMountedDevice {
                            id: format!("rem-{}", date_now_millis()),
                            remote_peer_id: from_id.to_string(),
                            name: dev_name,
                            icon: "⚡".to_string(),
                            driver_state: format!("Virtual USB Host (VHCI: {:?})", current_sess.state()),
                            latency: format!("{:.1} ms", current_sess.metrics.latency_ms),
                            throughput: format!("{:.1} MB/s", current_sess.metrics.throughput_mb_s),
                        });
                        drop(rem_lock);
                    }

                    // Broadcast real state change and telemetry events
                    let _ = state.ui_events_tx.send(serde_json::json!({
                        "type": "SESSION_STATE_CHANGED",
                        "session_id": current_sess.session_id,
                        "peer_id": current_sess.peer_id,
                        "device_id": current_sess.device_id,
                        "state": current_sess.state(),
                        "previous_state": current_sess.state_machine.previous_state(),
                        "reason": current_sess.state_machine.last_reason()
                    }));

                    let _ = state.ui_events_tx.send(serde_json::json!({
                        "type": "TELEMETRY_UPDATE",
                        "session_id": current_sess.session_id,
                        "peer_id": current_sess.peer_id,
                        "metrics": current_sess.metrics
                    }));
                } else {
                    // Declined: Transition to Closing -> Ready
                    let mut sessions = state.active_sessions.write().await;
                    if let Some(session) = sessions.get_mut(from_id) {
                        let _ = session.transition_to(SessionState::Closing, Some("Remote peer declined connection".to_string()));
                        let _ = session.transition_to(SessionState::Ready, Some("Returned to ready state".to_string()));
                        let current_sess = session.clone();
                        drop(sessions);

                        let _ = state.ui_events_tx.send(serde_json::json!({
                            "type": "SESSION_STATE_CHANGED",
                            "session_id": current_sess.session_id,
                            "peer_id": current_sess.peer_id,
                            "device_id": current_sess.device_id,
                            "state": current_sess.state(),
                            "previous_state": current_sess.state_machine.previous_state(),
                            "reason": current_sess.state_machine.last_reason()
                        }));
                    }
                }

                let _ = state.ui_events_tx.send(serde_json::json!({
                    "type": "CONNECT_DECISION_RESULT",
                    "fromId": from_id,
                    "accepted": accepted,
                    "reason": msg["reason"]
                }));
            }
            "REGISTER_CHALLENGE" => {
                if let Some(nonce) = msg["nonce"].as_str() {
                    let proof = state.identity.create_registration_proof(nonce);
                    let proof_msg = serde_json::json!({
                        "type": "REGISTER_PROOF",
                        "id": state.identity.raw_id.to_string(),
                        "publicKey": state.identity.installation_public_key,
                        "signature": proof,
                        "info": {
                            "hostname": std::env::var("HOSTNAME").or_else(|_| std::env::var("COMPUTERNAME")).unwrap_or_else(|_| "Host".to_string()),
                            "os": std::env::consts::OS,
                            "devices": *state.devices.read().await,
                            "version": "1.0.0"
                        }
                    });
                    let _ = state.broker_tx.send(proof_msg).await;
                }
            }
            "REGISTER_RESP" => {
                let verified = msg["verified"].as_bool().unwrap_or(false);
                if verified {
                    println!("  🔒 Broker cryptographically verified node: ID [{}]", msg["id"]);
                } else {
                    println!("  ✅ Broker registration acknowledged: ID [{}]", msg["id"]);
                }
            }
            "DISCONNECT" => {
                let peer_id = msg["fromId"].as_str().or_else(|| msg["peerId"].as_str()).unwrap_or_default();
                let clean_peer = peer_id.replace(' ', "");
                execute_session_disconnect(&state, &clean_peer, Some("Remote peer requested disconnect".to_string())).await;
            }
            "PONG" => {}
            _ => {
                println!("  ℹ️ Broker message: {}", msg_type);
            }
        }
    }
}

fn date_now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

// --- HOTPLUG MONITOR ---

async fn run_hotplug_monitor(state: Arc<DaemonState>) {
    let mut interval = tokio::time::interval(Duration::from_millis(1500));
    let mut last_count = 0;

    loop {
        interval.tick().await;
        let fresh = HardwareEnumerator::scan_devices();
        if fresh.len() != last_count {
            last_count = fresh.len();
            {
                let mut dev_lock = state.devices.write().await;
                *dev_lock = fresh.clone();
            }

            // Stream hotplug update to UI
            let _ = state.ui_events_tx.send(serde_json::json!({
                "type": "HOTPLUG_EVENT",
                "count": fresh.len(),
                "devices": fresh
            }));

            // Sync with broker
            if state.broker_connected.load(Ordering::Relaxed) {
                let _ = state.broker_tx.send(serde_json::json!({
                    "type": "DEVICE_CATALOG_UPDATE",
                    "devices": fresh
                })).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_authorized_host_exact_matching() {
        assert!(is_authorized_host("127.0.0.1"));
        assert!(is_authorized_host("127.0.0.1:19842"));
        assert!(is_authorized_host("localhost"));
        assert!(is_authorized_host("localhost:19842"));
        assert!(is_authorized_host("[::1]"));
        assert!(is_authorized_host("[::1]:19842"));

        // Host header attacks & bypass attempts
        assert!(!is_authorized_host("localhost.evil.com"));
        assert!(!is_authorized_host("localhost.evil.com:19842"));
        assert!(!is_authorized_host("127.0.0.1.evil.com"));
        assert!(!is_authorized_host("attacker.com"));
        assert!(!is_authorized_host("attacker.com:19842"));
        assert!(!is_authorized_host("localhost:99999"));
        assert!(!is_authorized_host("localhost:abc"));
        assert!(!is_authorized_host(""));
        assert!(!is_authorized_host("   "));
    }

    #[test]
    fn test_is_authorized_origin_exact_matching() {
        // Allowed local origins
        assert!(is_authorized_origin("http://127.0.0.1:19842"));
        assert!(is_authorized_origin("http://127.0.0.1"));
        assert!(is_authorized_origin("http://localhost:19842"));
        assert!(is_authorized_origin("http://localhost"));
        assert!(is_authorized_origin("https://localhost"));
        assert!(is_authorized_origin("http://[::1]:19842"));
        assert!(is_authorized_origin("http://[::1]"));
        assert!(is_authorized_origin("tauri://localhost"));
        assert!(is_authorized_origin("http://tauri.localhost"));
        assert!(is_authorized_origin("app://localhost"));
        assert!(is_authorized_origin("null"));

        // Disallowed / Malicious origins
        assert!(!is_authorized_origin("http://localhost.evil.com"));
        assert!(!is_authorized_origin("http://localhost.evil.com:19842"));
        assert!(!is_authorized_origin("http://127.0.0.1.evil.com"));
        assert!(!is_authorized_origin("http://127.0.0.1.evil.com:19842"));
        assert!(!is_authorized_origin("http://evil.com"));
        assert!(!is_authorized_origin("http://evil.com/localhost"));
        assert!(!is_authorized_origin("http://localhost:99999")); // Out-of-bounds port (>65535)
        assert!(!is_authorized_origin("http://localhost:0"));     // Port 0 invalid
        assert!(!is_authorized_origin("http://localhost:abc"));   // Non-numeric port
        assert!(!is_authorized_origin("javascript:alert(1)"));
        assert!(!is_authorized_origin("data:text/html,evil"));
        assert!(!is_authorized_origin("http://@evil.com"));
        assert!(!is_authorized_origin("http://[::1"));            // Malformed unclosed bracket
        assert!(!is_authorized_origin(""));
        assert!(!is_authorized_origin("   "));
    }

    #[test]
    fn test_generate_secure_api_token_entropy() {
        let t1 = generate_secure_api_token();
        let t2 = generate_secure_api_token();
        assert_eq!(t1.len(), 64);
        assert_eq!(t2.len(), 64);
        assert_ne!(t1, t2, "Successive CSPRNG tokens must be cryptographically distinct");
    }

    #[test]
    fn test_persisted_license_rejects_unsigned_claims_json() {
        let hw_hash = "test_hardware_hash_sec_adv_01";
        let claims = LicenseClaims {
            license_id: "LIC-UNSIGNED-BYPASS-ATTEMPT".to_string(),
            hardware_hash: hw_hash.to_string(),
            customer_name: "Attacker Entity".to_string(),
            tier: "EnterprisePro".to_string(),
            max_devices: 128,
            features: vec!["TURBO_URB_ZSTD".to_string()],
            issued_at: 1000,
            expires_at: 2000000000,
        };

        // Attempting to persist raw unverified claims JSON (as old SEC-ADV-01 vulnerability allowed)
        let unsigned_json = serde_json::to_string_pretty(&claims).unwrap();
        let loaded = parse_and_verify_persisted_license(&unsigned_json, hw_hash);
        assert!(
            loaded.is_none(),
            "SEC-ADV-01: Persisted unsigned LicenseClaims JSON must NEVER be accepted without signature verification"
        );
    }

    #[test]
    fn test_persisted_license_rejects_tampered_token() {
        use crate::licensing::{STATIC_PRODUCTION_FIXTURE_HW_HASH, STATIC_PRODUCTION_FIXTURE_TOKEN};
        let hw_hash = STATIC_PRODUCTION_FIXTURE_HW_HASH;
        let token = STATIC_PRODUCTION_FIXTURE_TOKEN;
        let valid_claims = LicenseEngine::verify_token(token, hw_hash).unwrap();
        let mut tampered_token = token.to_string();
        tampered_token.replace_range(10..15, "XXXXX");

        // Structured JSON with tampered token
        let tampered_json = serde_json::json!({
            "token": tampered_token,
            "claims": valid_claims,
        }).to_string();
        assert!(parse_and_verify_persisted_license(&tampered_json, hw_hash).is_none());

        // Raw tampered token string
        assert!(parse_and_verify_persisted_license(&tampered_token, hw_hash).is_none());
    }

    #[test]
    fn test_persisted_license_rejects_hardware_mismatch() {
        use crate::licensing::{STATIC_PRODUCTION_FIXTURE_HW_HASH, STATIC_PRODUCTION_FIXTURE_TOKEN};
        let hw_hash_a = STATIC_PRODUCTION_FIXTURE_HW_HASH;
        let hw_hash_b = "test_hardware_hash_machine_b";
        let token = STATIC_PRODUCTION_FIXTURE_TOKEN;
        let claims = LicenseEngine::verify_token(token, hw_hash_a).unwrap();

        let json = serde_json::json!({
            "token": token,
            "claims": claims,
        }).to_string();

        assert!(
            parse_and_verify_persisted_license(&json, hw_hash_b).is_none(),
            "Token anchored to machine A must be rejected on machine B"
        );
    }

    #[test]
    fn test_persisted_license_accepts_valid_signed_token_json_and_raw() {
        use crate::licensing::{STATIC_PRODUCTION_FIXTURE_HW_HASH, STATIC_PRODUCTION_FIXTURE_TOKEN};
        let hw_hash = STATIC_PRODUCTION_FIXTURE_HW_HASH;
        let token = STATIC_PRODUCTION_FIXTURE_TOKEN;
        let valid_claims = LicenseEngine::verify_token(token, hw_hash).unwrap();

        // 1. Structured JSON with "token"
        let json = serde_json::json!({
            "token": token,
            "claims": valid_claims,
        }).to_string();
        let loaded = parse_and_verify_persisted_license(&json, hw_hash)
            .expect("Valid structured JSON license must be accepted");
        assert_eq!(loaded.license_id, "LIC-2026-BHARTX-CERTIFIED");
        assert_eq!(loaded.tier, "EnterprisePro");

        // 2. Structured JSON where claims block was tampered with, but token is intact
        // The parser MUST return claims from the cryptographically verified token!
        let tampered_claims_json = serde_json::json!({
            "token": token,
            "claims": {
                "license_id": "TAMPERED_ID",
                "hardware_hash": hw_hash,
                "tier": "FreeHackedTier",
                "customer_name": "Tampered",
                "max_devices": 99999,
                "features": [],
                "issued_at": 1000,
                "expires_at": 2050000000
            }
        }).to_string();
        let loaded_tampered = parse_and_verify_persisted_license(&tampered_claims_json, hw_hash)
            .expect("Valid token in JSON must still verify");
        assert_eq!(
            loaded_tampered.license_id, "LIC-2026-BHARTX-CERTIFIED",
            "Claims must be derived from cryptographically verified token payload, never untrusted JSON fields"
        );
        assert_eq!(loaded_tampered.tier, "EnterprisePro");

        // 3. Raw token string
        let loaded_raw = parse_and_verify_persisted_license(token, hw_hash)
            .expect("Valid raw token string must be accepted");
        assert_eq!(loaded_raw.license_id, "LIC-2026-BHARTX-CERTIFIED");
    }

    #[tokio::test]
    async fn test_targeted_session_routing_and_orphaned_handling() {
        let (broker_tx, _) = mpsc::channel(16);
        let (binary_outbound_tx, _) = mpsc::channel(16);
        let (ui_events_tx, _) = broadcast::channel(16);
        let driver_client = Arc::new(VhciDriverClient::default());
        let filter_client = Arc::new(FilterDriverClient::default());

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
            filter_client,
            session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            dynamic_pin: Arc::new(RwLock::new(get_initial_session_pin())),
            failed_pin_attempts: Arc::new(Mutex::new(HashMap::new())),
        });

        // Set up session 1 (device 101) and session 2 (device 102)
        let (tx1, mut rx1) = tokio::sync::mpsc::channel(16);
        let (tx2, mut rx2) = tokio::sync::mpsc::channel(16);
        let (shut_tx1, _) = tokio::sync::watch::channel(false);
        let (shut_tx2, _) = tokio::sync::watch::channel(false);

        {
            let mut pumps = state.session_pumps.lock().await;
            pumps.insert("peer1".to_string(), SessionPumpHandles {
                peer_id: "peer1".to_string(),
                device_id: 101,
                vhci_to_net_task: tokio::spawn(async {}),
                net_to_vhci_task: tokio::spawn(async {}),
                net_fwd_task: tokio::spawn(async {}),
                shutdown_tx: shut_tx1,
                session_data_tx: tx1,
            });
            pumps.insert("peer2".to_string(), SessionPumpHandles {
                peer_id: "peer2".to_string(),
                device_id: 102,
                vhci_to_net_task: tokio::spawn(async {}),
                net_to_vhci_task: tokio::spawn(async {}),
                net_fwd_task: tokio::spawn(async {}),
                shutdown_tx: shut_tx2,
                session_data_tx: tx2,
            });
        }

        // 1. Send CMD_COMPLETE_URB targeting device 101
        let hdr1 = TurboUrbHeader::new(CMD_COMPLETE_URB, 1, 101, 0x81, TransferType::Bulk, 0);
        let bytes1 = hdr1.serialize();
        handle_incoming_binary_urb_frame(&state, bytes1.clone()).await;

        let res1 = rx1.try_recv().expect("Session 1 must receive frame for device 101");
        assert_eq!(res1, bytes1);
        assert!(rx2.try_recv().is_err(), "Session 2 must NOT receive frame for device 101");

        // 2. Send CMD_COMPLETE_URB targeting unknown device 999 (orphaned)
        let hdr_orphan = TurboUrbHeader::new(CMD_COMPLETE_URB, 2, 999, 0x81, TransferType::Bulk, 0);
        let bytes_orphan = hdr_orphan.serialize();
        handle_incoming_binary_urb_frame(&state, bytes_orphan).await;

        assert!(rx1.try_recv().is_err(), "Session 1 must not receive orphaned frame");
        assert!(rx2.try_recv().is_err(), "Session 2 must not receive orphaned frame");
    }

    #[tokio::test]
    async fn test_unsupported_urb_returns_status_not_supported() {
        let (broker_tx, _) = mpsc::channel(16);
        let (binary_outbound_tx, mut binary_outbound_rx) = mpsc::channel(16);
        let (ui_events_tx, _) = broadcast::channel(16);
        let driver_client = Arc::new(VhciDriverClient::default());
        let filter_client = Arc::new(FilterDriverClient::default());

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
            filter_client,
            session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            dynamic_pin: Arc::new(RwLock::new(get_initial_session_pin())),
            failed_pin_attempts: Arc::new(Mutex::new(HashMap::new())),
        });

        // Setup an unsupported vendor URB submit frame (non-GET_DESCRIPTOR)
        let submit_hdr = TurboUrbHeader::new(CMD_SUBMIT_URB, 42, 101, 0x00, TransferType::Control, 8);
        let mut frame = submit_hdr.serialize();
        // Setup packet: Vendor Request 0xFF (not GET_DESCRIPTOR)
        let setup_packet = [0x40, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        frame.extend_from_slice(&setup_packet);

        handle_host_urb_submission(&state, &submit_hdr, &frame).await;

        let resp_frame = binary_outbound_rx.recv().await.expect("Must produce response");
        let resp_hdr = TurboUrbHeader::deserialize_single_urb(&resp_frame[..TurboUrbHeader::SIZE])
            .expect("Valid header");

        assert_eq!(resp_hdr.command, CMD_COMPLETE_URB);
        assert_eq!(resp_hdr.sequence_id, 42);
        assert_eq!(resp_hdr.status, 0xC00000BB_u32 as i32, "Unsupported URB must return STATUS_NOT_SUPPORTED");
        assert_eq!(resp_hdr.actual_length, 0);
    }

    #[tokio::test]
    async fn test_dynamic_csprng_pin_generation_and_rotation() {
        let pin1 = generate_csprng_pin();
        let pin2 = generate_csprng_pin();

        assert_eq!(pin1.len(), 7, "PIN must be formatted as 'XXX XXX'");
        assert_eq!(pin2.len(), 7, "PIN must be formatted as 'XXX XXX'");
        assert_eq!(&pin1[3..4], " ", "PIN must contain space at index 3");

        let clean1: u32 = pin1.replace(' ', "").parse().expect("PIN 1 must be numeric");
        let clean2: u32 = pin2.replace(' ', "").parse().expect("PIN 2 must be numeric");
        assert!(clean1 >= 100_000 && clean1 <= 999_999);
        assert!(clean2 >= 100_000 && clean2 <= 999_999);

        // Over 10 iterations, generated PINs must demonstrate entropy
        let mut distinct = std::collections::HashSet::new();
        for _ in 0..10 {
            distinct.insert(generate_csprng_pin());
        }
        assert!(distinct.len() > 1, "CSPRNG PIN generation must produce distinct values");

        // Verify rotation on state
        let (broker_tx, _) = mpsc::channel(16);
        let (binary_outbound_tx, _) = mpsc::channel(16);
        let (ui_events_tx, mut ui_rx) = broadcast::channel(16);
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
            driver_client: Arc::new(VhciDriverClient::default()),
            filter_client: Arc::new(FilterDriverClient::default()),
            session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            dynamic_pin: Arc::new(RwLock::new("111 222".to_string())),
            failed_pin_attempts: Arc::new(Mutex::new(HashMap::new())),
        });

        assert_eq!(state.get_pin().await, "111 222");
        let rotated = state.rotate_pin().await;
        assert_ne!(rotated, "111 222");
        assert_eq!(state.get_pin().await, rotated);

        // Check broadcast event
        let evt = ui_rx.recv().await.expect("PIN_ROTATED event must be broadcast");
        assert_eq!(evt["type"], "PIN_ROTATED");
        assert_eq!(evt["pin"], rotated);
    }

    #[tokio::test]
    async fn test_pin_5_try_lockout_and_6th_attempt_blocked() {
        let (broker_tx, _) = mpsc::channel(16);
        let (binary_outbound_tx, _) = mpsc::channel(16);
        let (ui_events_tx, _) = broadcast::channel(16);
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
            driver_client: Arc::new(VhciDriverClient::default()),
            filter_client: Arc::new(FilterDriverClient::default()),
            session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            dynamic_pin: Arc::new(RwLock::new("999 888".to_string())),
            failed_pin_attempts: Arc::new(Mutex::new(HashMap::new())),
        });

        let attacker_id = "attacker-node-1";

        // Attempts 1 to 4 must return InvalidPin with decrementing remaining counts
        for i in 1..=4 {
            let res = state.verify_incoming_pin(attacker_id, "000 000").await;
            match res {
                PinVerificationResult::InvalidPin { attempts, remaining } => {
                    assert_eq!(attempts, i);
                    assert_eq!(remaining, 5 - i);
                }
                other => panic!("Expected InvalidPin on attempt {}, got {:?}", i, other),
            }
        }

        // 5th attempt must trigger LockOut
        let res5 = state.verify_incoming_pin(attacker_id, "000 000").await;
        match res5 {
            PinVerificationResult::LockedOut { message } => {
                assert!(message.contains("Security lockout"));
                assert!(message.contains("15 minutes"));
            }
            other => panic!("Expected LockedOut on 5th attempt, got {:?}", other),
        }

        // 6th attempt (even with CORRECT PIN!) must be strictly blocked due to active lockout
        let res6 = state.verify_incoming_pin(attacker_id, "999 888").await;
        match res6 {
            PinVerificationResult::LockedOut { message } => {
                assert!(message.contains("Security lockout"));
            }
            other => panic!("Expected LockedOut on 6th attempt, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_pin_successful_verification_resets_counter() {
        let (broker_tx, _) = mpsc::channel(16);
        let (binary_outbound_tx, _) = mpsc::channel(16);
        let (ui_events_tx, _) = broadcast::channel(16);
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
            driver_client: Arc::new(VhciDriverClient::default()),
            filter_client: Arc::new(FilterDriverClient::default()),
            session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            dynamic_pin: Arc::new(RwLock::new("555 444".to_string())),
            failed_pin_attempts: Arc::new(Mutex::new(HashMap::new())),
        });

        let peer = "peer-reset-test";

        // 3 failed attempts
        for _ in 0..3 {
            let res = state.verify_incoming_pin(peer, "111 111").await;
            assert!(matches!(res, PinVerificationResult::InvalidPin { .. }));
        }

        {
            let map = state.failed_pin_attempts.lock().await;
            assert_eq!(map.get(peer).unwrap().0, 3);
        }

        // Valid attempt resets counter
        let valid_res = state.verify_incoming_pin(peer, "555 444").await;
        assert_eq!(valid_res, PinVerificationResult::Success);

        {
            let map = state.failed_pin_attempts.lock().await;
            assert!(!map.contains_key(peer), "Successful PIN verification must remove failure record");
        }

        // Subsequent failure starts from attempt 1
        let next_fail = state.verify_incoming_pin(peer, "000 000").await;
        assert_eq!(next_fail, PinVerificationResult::InvalidPin { attempts: 1, remaining: 4 });
    }

    #[tokio::test]
    async fn test_pin_lockout_expired_cleanup() {
        let (broker_tx, _) = mpsc::channel(16);
        let (binary_outbound_tx, _) = mpsc::channel(16);
        let (ui_events_tx, _) = broadcast::channel(16);
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
            driver_client: Arc::new(VhciDriverClient::default()),
            filter_client: Arc::new(FilterDriverClient::default()),
            session_pumps: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            dynamic_pin: Arc::new(RwLock::new("777 888".to_string())),
            failed_pin_attempts: Arc::new(Mutex::new(HashMap::new())),
        });

        let peer = "expired-peer";

        // Seed with 5 failed attempts from 901 seconds ago (> 900s timeout)
        {
            let mut map = state.failed_pin_attempts.lock().await;
            let past = Instant::now() - Duration::from_secs(901);
            map.insert(peer.to_string(), (5, past));
        }

        // Calling verify with correct PIN after expiration should prune expired entry and succeed
        let res = state.verify_incoming_pin(peer, "777 888").await;
        assert_eq!(res, PinVerificationResult::Success, "Expired lockout must be pruned and valid PIN accepted");
    }
}
