use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Process information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
    pub is_running: bool,
}

/// Process manager for launching and managing apps/CLIs in the sandbox
pub struct ProcessManager;

impl ProcessManager {
    /// Launch a macOS .app by path
    pub fn spawn_app(app_path: &str) -> Result<ProcessInfo> {
        let _ = app_path;
        todo!("NSWorkspace.open() to launch .app")
    }

    /// Launch a CLI process with PTY support
    pub fn spawn_cli(command: &str, args: &[String]) -> Result<ProcessInfo> {
        let _ = (command, args);
        todo!("PTY process spawn")
    }

    /// List all running processes in the sandbox
    pub fn list_processes() -> Result<Vec<ProcessInfo>> {
        todo!("NSWorkspace runningApplications")
    }

    /// Kill a process by PID
    pub fn kill_process(pid: u32) -> Result<()> {
        let _ = pid;
        todo!("Process terminate")
    }

    /// Send input to a PTY process
    pub fn send_input(pid: u32, data: &[u8]) -> Result<()> {
        let _ = (pid, data);
        todo!("PTY write")
    }
}
