#![allow(unexpected_cfgs)]

pub mod automation;
pub mod capture;
pub mod diff;
pub mod instance;
pub mod player;
pub mod process;
pub mod recorder;
pub mod report;
pub mod sandbox;
pub mod scenario;
pub mod server;

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

        #[error("Bad request: {0}")]
        BadRequest(String),

        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),

        #[error("JSON error: {0}")]
        Json(#[from] serde_json::Error),

        #[error("Instance error: {0}")]
        Instance(String),
    }

    pub type Result<T> = std::result::Result<T, AppError>;
}
