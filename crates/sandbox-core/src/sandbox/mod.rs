use crate::capture::ScreenCapture;
use crate::error::{AppError, Result};
use crate::instance::InstanceKind;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub id: Option<String>,
    pub port: Option<u16>,
    pub mode: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub title: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            id: None,
            port: None,
            mode: None,
            command: None,
            args: Vec::new(),
            width: 1280,
            height: 800,
            title: "System Test Sandbox".to_string(),
        }
    }
}

/// A sub-window tracked by the sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubWindow {
    pub id: u32,
    pub title: String,
}

/// Sandbox window state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxState {
    pub sandbox_id: Option<String>,
    pub port: Option<u16>,
    pub window_id: Option<u32>,
    pub sub_windows: Vec<SubWindow>,
    pub width: u32,
    pub height: u32,
    pub is_running: bool,
}

/// The sandbox manages a dedicated window where target apps and CLIs run.
/// Screenshots are scoped to this window only using ScreenCaptureKit.
pub struct Sandbox {
    config: SandboxConfig,
    state: SandboxState,
    start_time: Option<Instant>,
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        let width = config.width;
        let height = config.height;
        let sandbox_id = config.id.clone();
        let port = config.port;
        Self {
            config,
            state: SandboxState {
                sandbox_id,
                port,
                window_id: None,
                sub_windows: Vec::new(),
                width,
                height,
                is_running: false,
            },
            start_time: None,
        }
    }

    /// Initialize the sandbox with a given window ID (set from Tauri after window creation).
    pub fn init(&mut self, window_id: u32) -> Result<()> {
        if window_id == 0 {
            return Err(AppError::WindowNotFound("Invalid window ID (0)".into()));
        }
        self.state.window_id = Some(window_id);
        self.state.is_running = true;
        self.start_time = Some(Instant::now());
        tracing::info!("Sandbox initialized with window_id={}", window_id);
        Ok(())
    }

    /// Set the window ID from an external source (e.g., Tauri window handle)
    pub fn set_window_id(&mut self, window_id: u32) {
        self.state.window_id = Some(window_id);
        self.state.is_running = true;
    }

    pub fn window_id(&self) -> Option<u32> {
        self.state.window_id
    }

    pub fn id(&self) -> Option<&str> {
        self.state.sandbox_id.as_deref()
    }

    pub fn port(&self) -> Option<u16> {
        self.state.port
    }

    pub fn kind(&self) -> Option<InstanceKind> {
        match (self.config.mode.as_deref(), &self.config.command) {
            (Some("cli"), Some(cmd)) => Some(InstanceKind::Cli {
                command: cmd.clone(),
                args: self.config.args.clone(),
            }),
            (Some("app"), Some(path)) => Some(InstanceKind::App { path: path.clone() }),
            _ => None,
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0)
    }

    /// Take a screenshot of only the sandbox window
    pub fn screenshot(&self) -> Result<Vec<u8>> {
        let window_id = self
            .state
            .window_id
            .ok_or(AppError::SandboxNotInitialized)?;
        ScreenCapture::capture_window(window_id)
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    pub fn state(&self) -> &SandboxState {
        &self.state
    }

    pub fn shutdown(&mut self) {
        self.state.is_running = false;
        self.state.window_id = None;
        self.state.sub_windows.clear();
        self.start_time = None;
        tracing::info!("Sandbox shut down");
    }

    pub fn add_window(&mut self, id: u32, title: String) {
        self.state.sub_windows.push(SubWindow { id, title });
    }

    pub fn remove_window(&mut self, id: u32) {
        self.state.sub_windows.retain(|w| w.id != id);
    }

    pub fn list_windows(&self) -> Vec<SubWindow> {
        let mut windows = self.state.sub_windows.clone();
        if let Some(main_id) = self.state.window_id {
            windows.insert(
                0,
                SubWindow {
                    id: main_id,
                    title: self.config.title.clone(),
                },
            );
        }
        windows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_new_default() {
        let sandbox = Sandbox::new(SandboxConfig::default());
        assert_eq!(sandbox.config().width, 1280);
        assert_eq!(sandbox.config().height, 800);
        assert!(sandbox.window_id().is_none());
        assert!(!sandbox.state().is_running);
        assert!(sandbox.id().is_none());
        assert!(sandbox.port().is_none());
    }

    #[test]
    fn test_sandbox_new_with_instance_config() {
        let config = SandboxConfig {
            id: Some("abc123".into()),
            port: Some(15801),
            mode: Some("cli".into()),
            command: Some("claude".into()),
            args: vec!["--help".into()],
            ..SandboxConfig::default()
        };
        let sandbox = Sandbox::new(config);
        assert_eq!(sandbox.id(), Some("abc123"));
        assert_eq!(sandbox.port(), Some(15801));
        let kind = sandbox.kind().unwrap();
        match kind {
            InstanceKind::Cli { command, args } => {
                assert_eq!(command, "claude");
                assert_eq!(args, vec!["--help"]);
            }
            _ => panic!("Expected CLI kind"),
        }
    }

    #[test]
    fn test_sandbox_init() {
        let mut sandbox = Sandbox::new(SandboxConfig::default());
        sandbox.init(42).unwrap();
        assert_eq!(sandbox.window_id(), Some(42));
        assert!(sandbox.state().is_running);
    }

    #[test]
    fn test_sandbox_init_invalid_id() {
        let mut sandbox = Sandbox::new(SandboxConfig::default());
        assert!(sandbox.init(0).is_err());
    }

    #[test]
    fn test_sandbox_shutdown() {
        let mut sandbox = Sandbox::new(SandboxConfig::default());
        sandbox.init(42).unwrap();
        sandbox.shutdown();
        assert!(!sandbox.state().is_running);
        assert!(sandbox.window_id().is_none());
    }

    #[test]
    fn test_screenshot_uninitialized() {
        let sandbox = Sandbox::new(SandboxConfig::default());
        assert!(sandbox.screenshot().is_err());
    }
}
