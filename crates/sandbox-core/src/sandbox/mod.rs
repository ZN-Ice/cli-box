use crate::capture::ScreenCapture;
use crate::error::Result;
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

/// Sandbox window state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxState {
    pub window_id: Option<u32>,
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
        Self {
            config,
            state: SandboxState {
                window_id: None,
                width: 0,
                height: 0,
                is_running: false,
            },
        }
    }

    /// Initialize the sandbox window
    pub fn init(&mut self) -> Result<()> {
        // TODO: Create NSWindow via Tauri or AppKit
        // Store window_id for ScreenCaptureKit targeting
        todo!("Create sandbox NSWindow")
    }

    /// Take a screenshot of only the sandbox window
    pub fn screenshot(&self) -> Result<Vec<u8>> {
        let window_id = self.state.window_id.ok_or(crate::error::AppError::SandboxNotInitialized)?;
        ScreenCapture::capture_window(window_id)
    }

    /// Get current sandbox state
    pub fn state(&self) -> &SandboxState {
        &self.state
    }
}
