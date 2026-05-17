use anyhow::Result;
use reqwest::Client;

pub struct SandboxClient {
    base_url: String,
    client: Client,
}

impl SandboxClient {
    pub fn new(port: u16) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            client: Client::new(),
        }
    }

    pub async fn health(&self) -> Result<bool> {
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let resp = self
            .client
            .get(format!("{}/screenshot", self.base_url))
            .send()
            .await?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    pub async fn click(&self, x: f64, y: f64, button: &str) -> Result<()> {
        self.client
            .post(format!("{}/input/click", self.base_url))
            .json(&serde_json::json!({"x": x, "y": y, "button": button}))
            .send()
            .await?;
        Ok(())
    }

    pub async fn type_text(&self, text: &str) -> Result<()> {
        self.client
            .post(format!("{}/input/type", self.base_url))
            .json(&serde_json::json!({"text": text}))
            .send()
            .await?;
        Ok(())
    }

    pub async fn press_key(&self, key: &str, modifiers: &[String]) -> Result<()> {
        self.client
            .post(format!("{}/input/key", self.base_url))
            .json(&serde_json::json!({"key": key, "modifiers": modifiers}))
            .send()
            .await?;
        Ok(())
    }

    pub async fn scroll(&self, x: f64, y: f64, direction: &str, amount: i32) -> Result<()> {
        self.client
            .post(format!("{}/input/scroll", self.base_url))
            .json(&serde_json::json!({"x": x, "y": y, "direction": direction, "amount": amount}))
            .send()
            .await?;
        Ok(())
    }

    pub async fn drag(&self, from_x: f64, from_y: f64, to_x: f64, to_y: f64) -> Result<()> {
        self.client
            .post(format!("{}/input/drag", self.base_url))
            .json(&serde_json::json!({"from_x": from_x, "from_y": from_y, "to_x": to_x, "to_y": to_y}))
            .send()
            .await?;
        Ok(())
    }

    pub async fn windows(&self) -> Result<Vec<(u32, String)>> {
        let resp = self
            .client
            .get(format!("{}/windows", self.base_url))
            .send()
            .await?;
        let windows = resp.json().await?;
        Ok(windows)
    }

    pub async fn processes(&self) -> Result<Vec<serde_json::Value>> {
        let resp = self
            .client
            .get(format!("{}/processes", self.base_url))
            .send()
            .await?;
        let procs = resp.json().await?;
        Ok(procs)
    }

    pub async fn spawn_app(&self, path: &str) -> Result<serde_json::Value> {
        let resp = self
            .client
            .post(format!("{}/app/spawn", self.base_url))
            .json(&serde_json::json!({"path": path}))
            .send()
            .await?;
        Ok(resp.json().await?)
    }

    pub async fn spawn_cli(&self, command: &str, args: &[String]) -> Result<serde_json::Value> {
        let resp = self
            .client
            .post(format!("{}/cli/spawn", self.base_url))
            .json(&serde_json::json!({"command": command, "args": args}))
            .send()
            .await?;
        Ok(resp.json().await?)
    }

    pub async fn kill_process(&self, pid: u32) -> Result<()> {
        self.client
            .post(format!("{}/process/kill", self.base_url))
            .json(&serde_json::json!({"pid": pid}))
            .send()
            .await?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.client
            .post(format!("{}/shutdown", self.base_url))
            .send()
            .await?;
        Ok(())
    }

    pub async fn pty_write(&self, pid: u32, data: &str) -> Result<()> {
        self.client
            .post(format!("{}/pty/write", self.base_url))
            .json(&serde_json::json!({"pid": pid, "data": data}))
            .send()
            .await?;
        Ok(())
    }

    pub async fn pty_read(&self, pid: u32) -> Result<Option<String>> {
        let resp = self
            .client
            .get(format!("{}/pty/output/{pid}", self.base_url))
            .send()
            .await?;
        let val: serde_json::Value = resp.json().await?;
        match val.get("output") {
            Some(v) if !v.is_null() => Ok(Some(v.as_str().unwrap_or_default().to_string())),
            _ => Ok(None),
        }
    }
}
