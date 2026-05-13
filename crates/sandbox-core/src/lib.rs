pub mod automation;
pub mod capture;
pub mod process;
pub mod sandbox;

pub use error::{AppError, Result};

mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum AppError {
        #[error("Window not found: {0}")]
        WindowNotFound(String),

        #[error("Process error: {0}")]
        Process(String),

        #[error("Screenshot failed: {0}")]
        Screenshot(String),

        #[error("Input injection failed: {0}")]
        Input(String),

        #[error("Accessibility error: {0}")]
        Accessibility(String),

        #[error("Sandbox not initialized")]
        SandboxNotInitialized,

        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),

        #[error("JSON error: {0}")]
        Json(#[from] serde_json::Error),
    }

    pub type Result<T> = std::result::Result<T, AppError>;
}
