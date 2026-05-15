use crate::capture::ScreenCapture;
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub width: u32,
    pub height: u32,
    pub title: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
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
    pub window_id: Option<u32>,
    /// Additional sub-windows tracked by the sandbox
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
}

impl Sandbox {
    pub fn new(config: SandboxConfig) -> Self {
        let width = config.width;
        let height = config.height;
        Self {
            config,
            state: SandboxState {
                window_id: None,
                sub_windows: Vec::new(),
                width,
                height,
                is_running: false,
            },
        }
    }

    /// Initialize the sandbox with a given window ID (set from Tauri after window creation).
    /// The window ID is used by ScreenCaptureKit to scope screenshots to this window only.
    pub fn init(&mut self, window_id: u32) -> Result<()> {
        if window_id == 0 {
            return Err(AppError::WindowNotFound("Invalid window ID (0)".into()));
        }
        self.state.window_id = Some(window_id);
        self.state.is_running = true;
        tracing::info!("Sandbox initialized with window_id={}", window_id);
        Ok(())
    }

    /// Set the window ID from an external source (e.g., Tauri window handle)
    pub fn set_window_id(&mut self, window_id: u32) {
        self.state.window_id = Some(window_id);
        self.state.is_running = true;
    }

    /// Get the current window ID if the sandbox is initialized
    pub fn window_id(&self) -> Option<u32> {
        self.state.window_id
    }

    /// Take a screenshot of only the sandbox window
    pub fn screenshot(&self) -> Result<Vec<u8>> {
        let window_id = self
            .state
            .window_id
            .ok_or(AppError::SandboxNotInitialized)?;
        ScreenCapture::capture_window(window_id)
    }

    /// Get the sandbox configuration
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Get current sandbox state
    pub fn state(&self) -> &SandboxState {
        &self.state
    }

    /// Shut down the sandbox
    pub fn shutdown(&mut self) {
        self.state.is_running = false;
        self.state.window_id = None;
        self.state.sub_windows.clear();
        tracing::info!("Sandbox shut down");
    }

    /// Add a sub-window to track
    pub fn add_window(&mut self, id: u32, title: String) {
        self.state.sub_windows.push(SubWindow { id, title });
    }

    /// Remove a sub-window by ID
    pub fn remove_window(&mut self, id: u32) {
        self.state.sub_windows.retain(|w| w.id != id);
    }

    /// List all tracked windows (main + sub)
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
    fn test_sandbox_new() {
        let sandbox = Sandbox::new(SandboxConfig::default());
        assert_eq!(sandbox.config().width, 1280);
        assert_eq!(sandbox.config().height, 800);
        assert!(sandbox.window_id().is_none());
        assert!(!sandbox.state().is_running);
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
