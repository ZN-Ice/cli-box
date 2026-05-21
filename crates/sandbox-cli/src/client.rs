use anyhow::{Context, Result};
use serde::Deserialize;

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

pub struct SandboxClient {
    base_url: String,
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
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            client: reqwest::Client::new(),
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
        self.client
            .post(format!("{}/pty/write", self.base_url))
            .json(&serde_json::json!({ "pid": pid, "data": data }))
            .send()
            .await
            .with_context(|| "pty_write request failed")?
            .error_for_status()
            .with_context(|| "pty_write failed")?;
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

/// Map a key name to its PTY byte representation.
pub fn key_to_pty_bytes(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "return" | "enter" => "\r".into(),
        "tab" => "\t".into(),
        "escape" | "esc" => "\x1b".into(),
        "backspace" | "delete" => "\x7f".into(),
        "space" => " ".into(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(key_to_pty_bytes("f1"), "");
    }

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
}
