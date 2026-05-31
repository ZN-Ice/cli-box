use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Check if debug logging is enabled via SANDBOX_LOGGER_LEVEL=debug
fn debug_enabled() -> bool {
    std::env::var("SANDBOX_LOGGER_LEVEL")
        .map(|v| v.to_lowercase() == "debug")
        .unwrap_or(false)
}

macro_rules! debug_log {
    ($($arg:tt)*) => {
        if debug_enabled() {
            eprintln!($($arg)*);
        }
    };
}

// ── Daemon discovery helpers ──────────────────────────────────

/// Resolve the daemon port from `daemon.json`. Errors if daemon is not running.
pub fn resolve_daemon_port() -> Result<u16> {
    sandbox_core::daemon::find_running_daemon()
        .with_context(|| "Sandbox daemon is not running. Start it with: sandbox start <command>")
}

/// Returns `http://127.0.0.1:{port}` for the running daemon.
pub fn daemon_base_url() -> Result<String> {
    let port = resolve_daemon_port()?;
    Ok(format!("http://127.0.0.1:{port}"))
}

// ── Daemon API response types ─────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DaemonHealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub sandboxes: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DaemonSandbox {
    pub id: String,
    pub kind: sandbox_core::instance::InstanceKind,
    pub status: sandbox_core::instance::InstanceStatus,
    pub port: u16,
    pub pty_pid: Option<u32>,
    pub window_id: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateSandboxResponse {
    pub sandbox_id: String,
    pub pty_pid: Option<u32>,
    pub window_id: Option<u32>,
}

// ── Daemon API commands ───────────────────────────────────────

/// Create a new sandbox via the daemon HTTP API.
pub async fn daemon_create_sandbox(
    mode: &str,
    command: Option<&str>,
    args: &[String],
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<CreateSandboxResponse> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let body = serde_json::json!({
        "mode": mode,
        "command": command,
        "args": args,
        "cols": cols,
        "rows": rows,
    });
    let resp = client
        .post(format!("{base}/sandbox/create"))
        .json(&body)
        .send()
        .await
        .with_context(|| "Failed to connect to sandbox daemon")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Daemon create failed (HTTP {status}): {text}");
    }
    let result: CreateSandboxResponse = resp.json().await?;
    Ok(result)
}

/// List all sandboxes via the daemon HTTP API.
pub async fn daemon_list_sandboxes() -> Result<Vec<DaemonSandbox>> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let resp = client
        .get(format!("{base}/sandbox/list"))
        .send()
        .await
        .with_context(|| "Failed to connect to sandbox daemon")?;
    let list: Vec<DaemonSandbox> = resp.json().await?;
    Ok(list)
}

/// Take a screenshot of a sandbox via the daemon HTTP API. Returns PNG bytes.
pub async fn daemon_screenshot(sandbox_id: &str) -> Result<Vec<u8>> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let resp = client
        .get(format!("{base}/sandbox/{sandbox_id}/screenshot"))
        .send()
        .await
        .with_context(|| "screenshot request to daemon failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("screenshot failed (HTTP {status}): {text}");
    }
    let bytes = resp.bytes().await?.to_vec();
    Ok(bytes)
}

/// Click in a sandbox via the daemon HTTP API.
pub async fn daemon_click(sandbox_id: &str, x: f64, y: f64, button: &str) -> Result<()> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    client
        .post(format!("{base}/sandbox/{sandbox_id}/input/click"))
        .json(&serde_json::json!({ "x": x, "y": y, "button": button }))
        .send()
        .await
        .with_context(|| "click request to daemon failed")?
        .error_for_status()
        .with_context(|| "click failed")?;
    Ok(())
}

/// Type text in a sandbox via the daemon HTTP API.
pub async fn daemon_type(sandbox_id: &str, text: &str) -> Result<()> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    client
        .post(format!("{base}/sandbox/{sandbox_id}/input/type"))
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await
        .with_context(|| "type request to daemon failed")?
        .error_for_status()
        .with_context(|| "type failed")?;
    Ok(())
}

/// Type text into a sandbox PTY via the daemon HTTP API.
pub async fn daemon_pty_write(sandbox_id: &str, data: &str) -> Result<()> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    client
        .post(format!("{base}/sandbox/{sandbox_id}/pty/write"))
        .json(&serde_json::json!({ "data": data }))
        .send()
        .await
        .with_context(|| "pty_write request to daemon failed")?
        .error_for_status()
        .with_context(|| "pty_write failed")?;
    Ok(())
}

/// Press a key in a sandbox via the daemon HTTP API.
pub async fn daemon_key(sandbox_id: &str, key: &str, modifiers: &[String]) -> Result<()> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    client
        .post(format!("{base}/sandbox/{sandbox_id}/input/key"))
        .json(&serde_json::json!({ "key": key, "modifiers": modifiers }))
        .send()
        .await
        .with_context(|| "key request to daemon failed")?
        .error_for_status()
        .with_context(|| "key failed")?;
    Ok(())
}

/// Close a sandbox via the daemon HTTP API.
pub async fn daemon_close(sandbox_id: &str) -> Result<()> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    client
        .post(format!("{base}/sandbox/{sandbox_id}/close"))
        .send()
        .await
        .with_context(|| "close request to daemon failed")?
        .error_for_status()
        .with_context(|| "close failed")?;
    Ok(())
}

/// Shut down the daemon via HTTP.
pub async fn daemon_shutdown() -> Result<()> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    client
        .post(format!("{base}/shutdown"))
        .send()
        .await
        .with_context(|| "shutdown request to daemon failed")?
        .error_for_status()
        .with_context(|| "daemon shutdown failed")?;
    Ok(())
}

/// Inspect a sandbox via the daemon API. Returns sandbox info from the daemon's in-memory state.
pub async fn daemon_inspect(sandbox_id: &str) -> Result<DaemonSandbox> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let resp = client
        .get(format!("{base}/sandbox/list"))
        .send()
        .await
        .with_context(|| "Failed to fetch sandbox list from daemon")?;
    let sandboxes: Vec<DaemonSandbox> = resp.json().await?;
    sandboxes
        .into_iter()
        .find(|sb| sb.id == sandbox_id)
        .with_context(|| format!("Sandbox '{sandbox_id}' not found in daemon"))
}

/// List processes in a sandbox via the daemon HTTP API.
pub async fn daemon_processes(sandbox_id: &str) -> Result<Vec<ProcessInfo>> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let resp = client
        .get(format!("{base}/sandbox/{sandbox_id}/processes"))
        .send()
        .await
        .with_context(|| "processes request to daemon failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("processes failed (HTTP {status}): {text}");
    }
    let processes: Vec<ProcessInfo> = resp.json().await?;
    Ok(processes)
}

/// Inspect UI tree of a sandbox window via the daemon HTTP API.
pub async fn daemon_ui_inspect(sandbox_id: &str) -> Result<serde_json::Value> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let resp = client
        .get(format!("{base}/sandbox/{sandbox_id}/ui/inspect"))
        .send()
        .await
        .with_context(|| "ui/inspect request to daemon failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("ui/inspect failed (HTTP {status}): {text}");
    }
    Ok(resp.json().await?)
}

/// Find UI elements by role/title in a sandbox window.
pub async fn daemon_ui_find(
    sandbox_id: &str,
    role: &str,
    title: Option<&str>,
) -> Result<serde_json::Value> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let mut body = serde_json::json!({ "role": role });
    if let Some(t) = title {
        body["title"] = serde_json::json!(t);
    }
    let resp = client
        .post(format!("{base}/sandbox/{sandbox_id}/ui/find"))
        .json(&body)
        .send()
        .await
        .with_context(|| "ui/find request to daemon failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("ui/find failed (HTTP {status}): {text}");
    }
    Ok(resp.json().await?)
}

/// Get the value of a UI element by its element ID.
pub async fn daemon_ui_value(sandbox_id: &str, element_id: &str) -> Result<serde_json::Value> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let resp = client
        .get(format!(
            "{base}/sandbox/{sandbox_id}/ui/value?element_id={element_id}"
        ))
        .send()
        .await
        .with_context(|| "ui/value request to daemon failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("ui/value failed (HTTP {status}): {text}");
    }
    Ok(resp.json().await?)
}

/// Set the window_id for a sandbox via the daemon HTTP API.
#[allow(dead_code)]
pub async fn daemon_set_window_id(sandbox_id: &str, window_id: u32) -> Result<()> {
    let base = daemon_base_url()?;
    let client = reqwest_client();
    let resp = client
        .post(format!("{base}/sandbox/{sandbox_id}/window"))
        .json(&serde_json::json!({ "window_id": window_id }))
        .send()
        .await
        .with_context(|| "set_window_id request to daemon failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("set_window_id failed (HTTP {status}): {text}");
    }
    Ok(())
}

fn reqwest_client() -> reqwest::Client {
    reqwest::ClientBuilder::new()
        .no_proxy()
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub sandbox_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct InfoResponse {
    pub sandbox_id: Option<String>,
    pub window_id: Option<u32>,
    pub uptime_secs: u64,
}

#[derive(Debug, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    pub is_running: bool,
}

#[derive(Debug, Deserialize)]
pub struct ReadyzResponse {
    pub status: String,
    #[allow(dead_code)]
    pub http_server: bool,
    pub pty_active: bool,
    pub pending_cli: bool,
}

pub struct SandboxClient {
    base_url: String,
    port: u16,
    client: reqwest::Client,
}

impl SandboxClient {
    pub fn from_instance_id(id: &str) -> Result<Self> {
        let registry = sandbox_core::instance::InstanceRegistry::default();
        let instance = registry.get(id).with_context(|| {
            format!("Instance '{id}' not found. Use 'sandbox list' to see running instances.")
        })?;
        tracing::info!(
            "Connecting to sandbox {} at port {}",
            instance.id,
            instance.port
        );
        Ok(Self::from_port(instance.port))
    }

    pub fn from_port(port: u16) -> Self {
        let client = reqwest::ClientBuilder::new()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            port,
            client,
        }
    }

    // ── Health & Info ──────────────────────────────────────

    pub async fn health(&self) -> Result<HealthResponse> {
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .with_context(|| "Failed to connect to sandbox")?;
        let health: HealthResponse = resp.json().await?;
        Ok(health)
    }

    pub async fn sandbox_info(&self) -> Result<InfoResponse> {
        let resp = self
            .client
            .get(format!("{}/sandbox/info", self.base_url))
            .send()
            .await
            .with_context(|| "Failed to get sandbox info")?;
        let info: InfoResponse = resp.json().await?;
        Ok(info)
    }

    pub async fn readyz(&self) -> Result<ReadyzResponse> {
        let resp = self
            .client
            .get(format!("{}/readyz", self.base_url))
            .send()
            .await
            .with_context(|| "Failed to connect to sandbox readyz")?;
        let readyz: ReadyzResponse = resp.json().await?;
        Ok(readyz)
    }

    // ── Input (CGEvent) ───────────────────────────────────

    pub async fn type_text(&self, text: &str) -> Result<()> {
        self.client
            .post(format!("{}/input/type", self.base_url))
            .json(&serde_json::json!({ "text": text }))
            .send()
            .await
            .with_context(|| "type_text request failed")?
            .error_for_status()
            .with_context(|| "type_text failed")?;
        Ok(())
    }

    pub async fn press_key(&self, key: &str, modifiers: &[String]) -> Result<()> {
        self.client
            .post(format!("{}/input/key", self.base_url))
            .json(&serde_json::json!({ "key": key, "modifiers": modifiers }))
            .send()
            .await
            .with_context(|| "press_key request failed")?
            .error_for_status()
            .with_context(|| "press_key failed")?;
        Ok(())
    }

    pub async fn click(&self, x: f64, y: f64, button: &str) -> Result<()> {
        self.client
            .post(format!("{}/input/click", self.base_url))
            .json(&serde_json::json!({ "x": x, "y": y, "button": button }))
            .send()
            .await
            .with_context(|| "click request failed")?
            .error_for_status()
            .with_context(|| "click failed")?;
        Ok(())
    }

    // ── Input (PTY) ───────────────────────────────────────

    pub async fn pty_write(&self, pid: u32, data: &str) -> Result<()> {
        use futures_util::SinkExt;
        use tokio_tungstenite::connect_async;

        let url = format!("ws://127.0.0.1:{}/pty/ws/{}", self.port, pid);
        debug_log!(
            "[DEBUG-CLI] pty_write: connecting to {}, data_len={}, data_preview={:?}",
            url,
            data.len(),
            if data.len() > 60 { &data[..60] } else { data }
        );
        debug_log!(
            "[DEBUG-CLI] pty_write: data_hex={}",
            data.bytes()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let (mut ws_stream, _) = connect_async(&url)
            .await
            .with_context(|| format!("Failed to connect to PTY WebSocket for pid={pid}"))?;
        debug_log!("[DEBUG-CLI] pty_write: WebSocket connected, sending message...");

        ws_stream
            .send(tokio_tungstenite::tungstenite::Message::Text(
                data.to_string().into(),
            ))
            .await
            .with_context(|| "Failed to send data to PTY WebSocket")?;
        debug_log!("[DEBUG-CLI] pty_write: message sent, waiting 100ms before close...");

        // Wait for the message to be delivered before closing
        // Without this delay, the WebSocket close handshake can race with
        // the server's message processing, causing the data to be lost.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        ws_stream.close(None).await.ok();
        debug_log!("[DEBUG-CLI] pty_write: done");

        Ok(())
    }

    pub async fn list_processes(&self) -> Result<Vec<ProcessInfo>> {
        let resp = self
            .client
            .get(format!("{}/processes", self.base_url))
            .send()
            .await
            .with_context(|| "list_processes request failed")?;
        let processes: Vec<ProcessInfo> = resp.json().await?;
        Ok(processes)
    }

    /// Write to PTY with auto-discovered PID (first process in the sandbox).
    pub async fn pty_write_auto(&self, data: &str) -> Result<()> {
        let processes = self.list_processes().await?;
        let first = processes
            .first()
            .with_context(|| "No PTY processes found in sandbox")?;
        self.pty_write(first.pid, data).await
    }

    // ── Screenshot ────────────────────────────────────────

    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(format!("{}/screenshot", self.base_url))
            .send()
            .await
            .with_context(|| "screenshot request failed")?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("screenshot failed (HTTP {status}): {body}");
        }
        let bytes = resp.bytes().await?.to_vec();
        Ok(bytes)
    }

    // ── Windows ────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn list_windows(&self) -> Result<Vec<(u32, String)>> {
        let resp = self
            .client
            .get(format!("{}/windows", self.base_url))
            .send()
            .await
            .with_context(|| "list_windows request failed")?;
        let windows: Vec<(u32, String)> = resp.json().await?;
        Ok(windows)
    }

    // ── Shutdown ───────────────────────────────────────────

    pub async fn shutdown(&self) -> Result<()> {
        self.client
            .post(format!("{}/shutdown", self.base_url))
            .send()
            .await
            .with_context(|| "shutdown request failed")?
            .error_for_status()
            .with_context(|| "shutdown failed")?;
        Ok(())
    }
}

/// Map a key name to its PTY byte representation (terminal escape sequences).
pub fn key_to_pty_bytes(key: &str) -> String {
    match key.to_lowercase().as_str() {
        // Basic control keys
        "return" | "enter" => "\r".into(),
        "tab" => "\t".into(),
        "escape" | "esc" => "\x1b".into(),
        "backspace" | "delete" => "\x7f".into(),
        "space" => " ".into(),

        // Arrow keys (ANSI escape sequences)
        "up" | "arrowup" => "\x1b[A".into(),
        "down" | "arrowdown" => "\x1b[B".into(),
        "right" | "arrowright" => "\x1b[C".into(),
        "left" | "arrowleft" => "\x1b[D".into(),

        // Navigation keys
        "home" => "\x1b[H".into(),
        "end" => "\x1b[F".into(),
        "pageup" | "page_up" => "\x1b[5~".into(),
        "pagedown" | "page_down" => "\x1b[6~".into(),
        "insert" => "\x1b[2~".into(),

        // Function keys
        "f1" => "\x1bOP".into(),
        "f2" => "\x1bOQ".into(),
        "f3" => "\x1bOR".into(),
        "f4" => "\x1bOS".into(),
        "f5" => "\x1b[15~".into(),
        "f6" => "\x1b[17~".into(),
        "f7" => "\x1b[18~".into(),
        "f8" => "\x1b[19~".into(),
        "f9" => "\x1b[20~".into(),
        "f10" => "\x1b[21~".into(),
        "f11" => "\x1b[23~".into(),
        "f12" => "\x1b[24~".into(),

        // Ctrl+letter combinations
        "ctrl+a" => "\x01".into(),
        "ctrl+b" => "\x02".into(),
        "ctrl+c" => "\x03".into(),
        "ctrl+d" => "\x04".into(),
        "ctrl+e" => "\x05".into(),
        "ctrl+f" => "\x06".into(),
        "ctrl+g" => "\x07".into(),
        "ctrl+h" => "\x08".into(),
        "ctrl+i" => "\x09".into(),
        "ctrl+j" => "\x0a".into(),
        "ctrl+k" => "\x0b".into(),
        "ctrl+l" => "\x0c".into(),
        "ctrl+m" => "\x0d".into(),
        "ctrl+n" => "\x0e".into(),
        "ctrl+o" => "\x0f".into(),
        "ctrl+p" => "\x10".into(),
        "ctrl+q" => "\x11".into(),
        "ctrl+r" => "\x12".into(),
        "ctrl+s" => "\x13".into(),
        "ctrl+t" => "\x14".into(),
        "ctrl+u" => "\x15".into(),
        "ctrl+v" => "\x16".into(),
        "ctrl+w" => "\x17".into(),
        "ctrl+x" => "\x18".into(),
        "ctrl+y" => "\x19".into(),
        "ctrl+z" => "\x1a".into(),

        _ => String::new(),
    }
}

/// Map a key + modifiers to PTY bytes, handling modifier-enhanced sequences.
pub fn key_to_pty_bytes_with_modifiers(key: &str, modifiers: &[String]) -> String {
    // Check for ctrl+letter shorthand first
    let key_lower = key.to_lowercase();
    if key_lower.starts_with("ctrl+") {
        return key_to_pty_bytes(&key_lower);
    }

    // Handle modifier-enhanced keys
    let has_ctrl = modifiers
        .iter()
        .any(|m| m.to_lowercase() == "ctrl" || m.to_lowercase() == "control");
    let has_shift = modifiers.iter().any(|m| m.to_lowercase() == "shift");
    let has_alt = modifiers
        .iter()
        .any(|m| m.to_lowercase() == "alt" || m.to_lowercase() == "option");

    let key_match = key_lower.as_str();

    // Ctrl + key combinations
    if has_ctrl {
        if let Some(c) = key_match.chars().next() {
            if c.is_ascii_lowercase() {
                let byte = (c as u8) - b'a' + 1;
                return (byte as char).to_string();
            }
            if c.is_ascii_uppercase() {
                let byte = (c as u8) - b'A' + 1;
                return (byte as char).to_string();
            }
            // Special Ctrl combinations
            return match key_match {
                "[" => "\x1b".into(),
                "]" => "\x1d".into(),
                "\\" => "\x1c".into(),
                _ => String::new(),
            };
        }
    }

    // Alt/Option + key (ESC prefix)
    if has_alt {
        if let Some(c) = key.chars().next() {
            return format!("\x1b{}", c);
        }
    }

    // Shift + arrow keys (select mode)
    if has_shift {
        match key_match {
            "up" | "arrowup" => return "\x1b[1;2A".into(),
            "down" | "arrowdown" => return "\x1b[1;2B".into(),
            "right" | "arrowright" => return "\x1b[1;2C".into(),
            "left" | "arrowleft" => return "\x1b[1;2D".into(),
            "tab" => return "\x1b[Z".into(), // Shift+Tab
            _ => {}
        }
    }

    // Plain key fallback
    key_to_pty_bytes(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Basic key mappings ───────────────────────────────────

    #[test]
    fn test_key_to_pty_bytes_return() {
        assert_eq!(key_to_pty_bytes("return"), "\r");
        assert_eq!(key_to_pty_bytes("Return"), "\r");
        assert_eq!(key_to_pty_bytes("enter"), "\r");
        assert_eq!(key_to_pty_bytes("ENTER"), "\r");
    }

    #[test]
    fn test_key_to_pty_bytes_tab() {
        assert_eq!(key_to_pty_bytes("tab"), "\t");
        assert_eq!(key_to_pty_bytes("Tab"), "\t");
    }

    #[test]
    fn test_key_to_pty_bytes_escape() {
        assert_eq!(key_to_pty_bytes("escape"), "\x1b");
        assert_eq!(key_to_pty_bytes("esc"), "\x1b");
    }

    #[test]
    fn test_key_to_pty_bytes_backspace() {
        assert_eq!(key_to_pty_bytes("backspace"), "\x7f");
        assert_eq!(key_to_pty_bytes("delete"), "\x7f");
    }

    #[test]
    fn test_key_to_pty_bytes_space() {
        assert_eq!(key_to_pty_bytes("space"), " ");
    }

    #[test]
    fn test_key_to_pty_bytes_unknown() {
        assert_eq!(key_to_pty_bytes("a"), "");
        assert_eq!(key_to_pty_bytes("f13"), "");
    }

    // ── Arrow keys ──────────────────────────────────────────

    #[test]
    fn test_key_to_pty_bytes_arrow_keys() {
        assert_eq!(key_to_pty_bytes("up"), "\x1b[A");
        assert_eq!(key_to_pty_bytes("down"), "\x1b[B");
        assert_eq!(key_to_pty_bytes("right"), "\x1b[C");
        assert_eq!(key_to_pty_bytes("left"), "\x1b[D");
        assert_eq!(key_to_pty_bytes("Up"), "\x1b[A");
        assert_eq!(key_to_pty_bytes("arrowup"), "\x1b[A");
    }

    // ── Navigation keys ─────────────────────────────────────

    #[test]
    fn test_key_to_pty_bytes_navigation() {
        assert_eq!(key_to_pty_bytes("home"), "\x1b[H");
        assert_eq!(key_to_pty_bytes("end"), "\x1b[F");
        assert_eq!(key_to_pty_bytes("pageup"), "\x1b[5~");
        assert_eq!(key_to_pty_bytes("pagedown"), "\x1b[6~");
        assert_eq!(key_to_pty_bytes("page_up"), "\x1b[5~");
        assert_eq!(key_to_pty_bytes("page_down"), "\x1b[6~");
        assert_eq!(key_to_pty_bytes("insert"), "\x1b[2~");
    }

    // ── Function keys ───────────────────────────────────────

    #[test]
    fn test_key_to_pty_bytes_fkeys() {
        assert_eq!(key_to_pty_bytes("f1"), "\x1bOP");
        assert_eq!(key_to_pty_bytes("f2"), "\x1bOQ");
        assert_eq!(key_to_pty_bytes("f3"), "\x1bOR");
        assert_eq!(key_to_pty_bytes("f4"), "\x1bOS");
        assert_eq!(key_to_pty_bytes("f5"), "\x1b[15~");
        assert_eq!(key_to_pty_bytes("f6"), "\x1b[17~");
        assert_eq!(key_to_pty_bytes("f7"), "\x1b[18~");
        assert_eq!(key_to_pty_bytes("f8"), "\x1b[19~");
        assert_eq!(key_to_pty_bytes("f9"), "\x1b[20~");
        assert_eq!(key_to_pty_bytes("f10"), "\x1b[21~");
        assert_eq!(key_to_pty_bytes("f11"), "\x1b[23~");
        assert_eq!(key_to_pty_bytes("f12"), "\x1b[24~");
    }

    // ── Ctrl+letter combinations ─────────────────────────────

    #[test]
    fn test_key_to_pty_bytes_ctrl_letters() {
        assert_eq!(key_to_pty_bytes("ctrl+c"), "\x03");
        assert_eq!(key_to_pty_bytes("ctrl+d"), "\x04");
        assert_eq!(key_to_pty_bytes("ctrl+z"), "\x1a");
        assert_eq!(key_to_pty_bytes("ctrl+c"), "\x03");
        assert_eq!(key_to_pty_bytes("ctrl+l"), "\x0c");
        assert_eq!(key_to_pty_bytes("ctrl+r"), "\x12");
        assert_eq!(key_to_pty_bytes("ctrl+a"), "\x01");
        assert_eq!(key_to_pty_bytes("ctrl+e"), "\x05");
        assert_eq!(key_to_pty_bytes("ctrl+w"), "\x17");
        assert_eq!(key_to_pty_bytes("ctrl+u"), "\x15");
        assert_eq!(key_to_pty_bytes("ctrl+k"), "\x0b");
        assert_eq!(key_to_pty_bytes("ctrl+p"), "\x10");
        assert_eq!(key_to_pty_bytes("ctrl+n"), "\x0e");
    }

    // ── key_to_pty_bytes_with_modifiers ──────────────────────

    #[test]
    fn test_with_modifiers_ctrl_key() {
        assert_eq!(
            key_to_pty_bytes_with_modifiers("c", &["ctrl".into()]),
            "\x03"
        );
        assert_eq!(
            key_to_pty_bytes_with_modifiers("d", &["ctrl".into()]),
            "\x04"
        );
        assert_eq!(
            key_to_pty_bytes_with_modifiers("z", &["control".into()]),
            "\x1a"
        );
    }

    #[test]
    fn test_with_modifiers_ctrl_uppercase() {
        assert_eq!(
            key_to_pty_bytes_with_modifiers("C", &["ctrl".into()]),
            "\x03"
        );
    }

    #[test]
    fn test_with_modifiers_alt_key() {
        assert_eq!(
            key_to_pty_bytes_with_modifiers("a", &["alt".into()]),
            "\x1ba"
        );
        assert_eq!(
            key_to_pty_bytes_with_modifiers("x", &["option".into()]),
            "\x1bx"
        );
    }

    #[test]
    fn test_with_modifiers_shift_arrow() {
        assert_eq!(
            key_to_pty_bytes_with_modifiers("up", &["shift".into()]),
            "\x1b[1;2A"
        );
        assert_eq!(
            key_to_pty_bytes_with_modifiers("down", &["shift".into()]),
            "\x1b[1;2B"
        );
        assert_eq!(
            key_to_pty_bytes_with_modifiers("right", &["shift".into()]),
            "\x1b[1;2C"
        );
        assert_eq!(
            key_to_pty_bytes_with_modifiers("left", &["shift".into()]),
            "\x1b[1;2D"
        );
    }

    #[test]
    fn test_with_modifiers_shift_tab() {
        assert_eq!(
            key_to_pty_bytes_with_modifiers("tab", &["shift".into()]),
            "\x1b[Z"
        );
    }

    #[test]
    fn test_with_modifiers_ctrl_bracket() {
        assert_eq!(
            key_to_pty_bytes_with_modifiers("[", &["ctrl".into()]),
            "\x1b"
        );
        assert_eq!(
            key_to_pty_bytes_with_modifiers("]", &["ctrl".into()]),
            "\x1d"
        );
    }

    #[test]
    fn test_with_modifiers_ctrl_shorthand() {
        // "ctrl+c" as the key itself should work
        assert_eq!(key_to_pty_bytes_with_modifiers("ctrl+c", &[]), "\x03");
    }

    #[test]
    fn test_with_modifiers_plain_key_fallback() {
        assert_eq!(key_to_pty_bytes_with_modifiers("return", &[]), "\r");
        assert_eq!(key_to_pty_bytes_with_modifiers("up", &[]), "\x1b[A");
        assert_eq!(key_to_pty_bytes_with_modifiers("f1", &[]), "\x1bOP");
    }

    #[test]
    fn test_with_modifiers_unknown_key() {
        assert_eq!(key_to_pty_bytes_with_modifiers("a", &[]), "");
    }

    // ── Claude-specific interaction sequences ────────────────

    #[test]
    fn test_claude_typical_interactions() {
        // These are the key sequences Claude Code commonly needs

        // Submit input: Return
        assert_eq!(key_to_pty_bytes("return"), "\r");

        // Cancel current operation: Ctrl+C
        assert_eq!(key_to_pty_bytes("ctrl+c"), "\x03");

        // Exit: Ctrl+C or Ctrl+D
        assert_eq!(key_to_pty_bytes("ctrl+d"), "\x04");

        // Navigate history: Up/Down
        assert_eq!(key_to_pty_bytes("up"), "\x1b[A");
        assert_eq!(key_to_pty_bytes("down"), "\x1b[B");

        // Autocomplete: Tab
        assert_eq!(key_to_pty_bytes("tab"), "\t");

        // Clear screen: Ctrl+L
        assert_eq!(key_to_pty_bytes("ctrl+l"), "\x0c");

        // Accept autocomplete: Right arrow
        assert_eq!(key_to_pty_bytes("right"), "\x1b[C");

        // Search history: Ctrl+R
        assert_eq!(key_to_pty_bytes("ctrl+r"), "\x12");

        // With modifiers via function
        assert_eq!(
            key_to_pty_bytes_with_modifiers("c", &["ctrl".into()]),
            "\x03"
        );
    }

    // ── Existing tests ──────────────────────────────────────

    #[test]
    fn test_deserialize_health_response() {
        let json = r#"{"status":"ok","version":"0.2.0","uptime_secs":42,"sandbox_id":"abc123"}"#;
        let resp: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.sandbox_id, Some("abc123".into()));
    }

    #[test]
    fn test_deserialize_info_response() {
        let json = r#"{"sandbox_id":"abc","window_id":42,"uptime_secs":60}"#;
        let resp: InfoResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.window_id, Some(42));
    }

    #[test]
    fn test_deserialize_process_info() {
        let json = r#"{"pid":1001,"name":"claude","path":null,"is_running":true}"#;
        let info: ProcessInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.pid, 1001);
        assert_eq!(info.name, "claude");
        assert!(info.is_running);
    }

    #[test]
    fn test_from_instance_id_not_found() {
        let result = SandboxClient::from_instance_id("nonexistent_id_12345");
        assert!(result.is_err());
    }

    // ── ReadyzResponse deserialization ──────────────────────

    #[test]
    fn test_deserialize_readyz_ready() {
        let json = r#"{"status":"ready","http_server":true,"pty_active":true,"pending_cli":false}"#;
        let resp: ReadyzResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ready");
        assert!(resp.http_server);
        assert!(resp.pty_active);
        assert!(!resp.pending_cli);
    }

    #[test]
    fn test_deserialize_readyz_not_ready() {
        let json =
            r#"{"status":"not_ready","http_server":true,"pty_active":false,"pending_cli":false}"#;
        let resp: ReadyzResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "not_ready");
        assert!(!resp.pty_active);
        assert!(!resp.pending_cli);
    }

    #[test]
    fn test_deserialize_readyz_pending_cli() {
        let json = r#"{"status":"ready","http_server":true,"pty_active":false,"pending_cli":true}"#;
        let resp: ReadyzResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ready");
        assert!(resp.pending_cli);
    }
}
