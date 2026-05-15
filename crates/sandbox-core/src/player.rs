use crate::automation::cg_event::{InputSimulator, MouseButton};
use crate::capture::ScreenCapture;
use crate::diff::{diff_images, DiffOptions};
use crate::error::{AppError, Result};
use crate::process::ProcessManager;
use crate::recorder::Action;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Plays back recorded actions from a JSONL file or Vec<Action>
pub struct ActionPlayer {
    /// Speed multiplier (1.0 = original speed, 2.0 = 2x speed)
    speed: f64,
    /// Screenshots taken during playback, keyed by label
    screenshots: HashMap<String, Vec<u8>>,
}

/// Result of playing back a single action
#[derive(Debug)]
pub enum ActionResult {
    Ok,
    Screenshot {
        label: String,
        data: Vec<u8>,
    },
    DiffResult {
        label: String,
        diff: crate::diff::DiffResult,
    },
    Error {
        message: String,
    },
}

impl ActionPlayer {
    pub fn new(speed: f64) -> Self {
        Self {
            speed: speed.max(0.1),
            screenshots: HashMap::new(),
        }
    }

    /// Load actions from a JSONL file
    pub fn load_file(path: &Path) -> Result<Vec<Action>> {
        let content = std::fs::read_to_string(path)?;
        let mut actions = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let action: Action = serde_json::from_str(line).map_err(|e| {
                AppError::Screenshot(format!("Failed to parse action: {} — {}", e, line))
            })?;
            actions.push(action);
        }
        Ok(actions)
    }

    /// Play back a sequence of actions
    #[cfg(target_os = "macos")]
    pub async fn play(&mut self, actions: &[Action]) -> Vec<ActionResult> {
        let mut results = Vec::new();
        let mut last_timestamp: u64 = 0;

        for action in actions {
            // Calculate wait time based on timestamp difference
            let ts = self.get_timestamp(action);
            if ts > last_timestamp {
                let wait_ms = ((ts - last_timestamp) as f64 / self.speed) as u64;
                if wait_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                }
            }
            last_timestamp = ts;

            let result = self.execute(action).await;
            results.push(result);
        }

        results
    }

    #[cfg(not(target_os = "macos"))]
    pub async fn play(&mut self, _actions: &[Action]) -> Vec<ActionResult> {
        vec![ActionResult::Error {
            message: "Playback only available on macOS".to_string(),
        }]
    }

    fn get_timestamp(&self, action: &Action) -> u64 {
        match action {
            Action::Click { timestamp_ms, .. }
            | Action::DoubleClick { timestamp_ms, .. }
            | Action::TypeText { timestamp_ms, .. }
            | Action::PressKey { timestamp_ms, .. }
            | Action::Scroll { timestamp_ms, .. }
            | Action::Drag { timestamp_ms, .. }
            | Action::Screenshot { timestamp_ms, .. }
            | Action::SpawnApp { timestamp_ms, .. }
            | Action::SpawnCli { timestamp_ms, .. }
            | Action::Wait { timestamp_ms, .. }
            | Action::AssertScreenshot { timestamp_ms, .. } => timestamp_ms.unwrap_or(0),
        }
    }

    #[cfg(target_os = "macos")]
    async fn execute(&mut self, action: &Action) -> ActionResult {
        match action {
            Action::Click { x, y, button, .. } => {
                let btn = match button.to_lowercase().as_str() {
                    "right" => MouseButton::Right,
                    "middle" => MouseButton::Middle,
                    _ => MouseButton::Left,
                };
                match InputSimulator::click(*x, *y, btn) {
                    Ok(()) => ActionResult::Ok,
                    Err(e) => ActionResult::Error {
                        message: e.to_string(),
                    },
                }
            }
            Action::DoubleClick { x, y, .. } => match InputSimulator::double_click(*x, *y) {
                Ok(()) => ActionResult::Ok,
                Err(e) => ActionResult::Error {
                    message: e.to_string(),
                },
            },
            Action::TypeText { text, .. } => match InputSimulator::type_text(text) {
                Ok(()) => ActionResult::Ok,
                Err(e) => ActionResult::Error {
                    message: e.to_string(),
                },
            },
            Action::PressKey { key, modifiers, .. } => {
                let m: Vec<&str> = modifiers.iter().map(|s| s.as_str()).collect();
                match InputSimulator::press_key(key, &m) {
                    Ok(()) => ActionResult::Ok,
                    Err(e) => ActionResult::Error {
                        message: e.to_string(),
                    },
                }
            }
            Action::Scroll {
                x,
                y,
                direction,
                amount,
                ..
            } => match InputSimulator::scroll(*x, *y, direction, *amount) {
                Ok(()) => ActionResult::Ok,
                Err(e) => ActionResult::Error {
                    message: e.to_string(),
                },
            },
            Action::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
                ..
            } => match InputSimulator::drag(*from_x, *from_y, *to_x, *to_y) {
                Ok(()) => ActionResult::Ok,
                Err(e) => ActionResult::Error {
                    message: e.to_string(),
                },
            },
            Action::Screenshot { label, .. } => match ScreenCapture::capture_sandbox() {
                Ok(data) => {
                    if let Some(lbl) = label {
                        self.screenshots.insert(lbl.clone(), data.clone());
                    }
                    ActionResult::Screenshot {
                        label: label.clone().unwrap_or_default(),
                        data,
                    }
                }
                Err(e) => ActionResult::Error {
                    message: e.to_string(),
                },
            },
            Action::SpawnApp { path, .. } => match ProcessManager::spawn_app(path) {
                Ok(_) => ActionResult::Ok,
                Err(e) => ActionResult::Error {
                    message: e.to_string(),
                },
            },
            Action::SpawnCli { command, args, .. } => {
                match ProcessManager::spawn_cli(command, args) {
                    Ok(_) => ActionResult::Ok,
                    Err(e) => ActionResult::Error {
                        message: e.to_string(),
                    },
                }
            }
            Action::Wait { duration_ms, .. } => {
                tokio::time::sleep(Duration::from_millis(*duration_ms)).await;
                ActionResult::Ok
            }
            Action::AssertScreenshot {
                label,
                max_diff_percentage,
                ..
            } => {
                // Capture current screenshot and compare with stored one
                match ScreenCapture::capture_sandbox() {
                    Ok(current) => {
                        if let Some(lbl) = label {
                            if let Some(expected) = self.screenshots.get(lbl) {
                                let options = DiffOptions {
                                    max_diff_percentage: *max_diff_percentage,
                                    ..Default::default()
                                };
                                match diff_images(expected, &current, &options) {
                                    Ok(diff) => ActionResult::DiffResult {
                                        label: lbl.clone(),
                                        diff,
                                    },
                                    Err(e) => ActionResult::Error {
                                        message: e.to_string(),
                                    },
                                }
                            } else {
                                ActionResult::Error {
                                    message: format!(
                                        "No reference screenshot found for label: {}",
                                        lbl
                                    ),
                                }
                            }
                        } else {
                            ActionResult::Error {
                                message: "AssertScreenshot requires a label".to_string(),
                            }
                        }
                    }
                    Err(e) => ActionResult::Error {
                        message: e.to_string(),
                    },
                }
            }
        }
    }

    /// Get captured screenshots
    pub fn screenshots(&self) -> &HashMap<String, Vec<u8>> {
        &self.screenshots
    }
}
