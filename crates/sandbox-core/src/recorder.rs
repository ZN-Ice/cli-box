use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// A recorded user action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Click {
        x: f64,
        y: f64,
        button: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    DoubleClick {
        x: f64,
        y: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    TypeText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    PressKey {
        key: String,
        modifiers: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    Scroll {
        x: f64,
        y: f64,
        direction: String,
        amount: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    Drag {
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    Screenshot {
        label: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    SpawnApp {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    SpawnCli {
        command: String,
        args: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    Wait {
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
    AssertScreenshot {
        label: Option<String>,
        max_diff_percentage: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp_ms: Option<u64>,
    },
}

/// Records user actions, flushing to a JSONL file
pub struct ActionRecorder {
    actions: Mutex<Vec<Action>>,
    output_path: Mutex<Option<PathBuf>>,
    start_time: SystemTime,
    enabled: Mutex<bool>,
}

impl ActionRecorder {
    pub fn new() -> Self {
        Self {
            actions: Mutex::new(Vec::new()),
            output_path: Mutex::new(None),
            start_time: SystemTime::now(),
            enabled: Mutex::new(false),
        }
    }

    /// Start recording. If a path is provided, also flush to file.
    pub fn start(&self, output_path: Option<PathBuf>) -> Result<()> {
        let mut enabled = self.enabled.lock().unwrap();
        *enabled = true;
        if let Some(path) = output_path {
            *self.output_path.lock().unwrap() = Some(path);
        }
        *self.actions.lock().unwrap() = Vec::new();
        Ok(())
    }

    /// Stop recording
    pub fn stop(&self) -> Result<Vec<Action>> {
        let mut enabled = self.enabled.lock().unwrap();
        *enabled = false;
        let actions = self.actions.lock().unwrap().clone();
        Ok(actions)
    }

    /// Record an action
    pub fn record(&self, action: Action) -> Result<()> {
        if !*self.enabled.lock().unwrap() {
            return Ok(());
        }
        let elapsed = self.start_time.elapsed().unwrap_or(Duration::ZERO);
        let timestamp_ms = elapsed.as_millis() as u64;

        let mut action = action;
        // Set timestamp on the action
        match &mut action {
            Action::Click {
                timestamp_ms: ts, ..
            }
            | Action::DoubleClick {
                timestamp_ms: ts, ..
            }
            | Action::TypeText {
                timestamp_ms: ts, ..
            }
            | Action::PressKey {
                timestamp_ms: ts, ..
            }
            | Action::Scroll {
                timestamp_ms: ts, ..
            }
            | Action::Drag {
                timestamp_ms: ts, ..
            }
            | Action::Screenshot {
                timestamp_ms: ts, ..
            }
            | Action::SpawnApp {
                timestamp_ms: ts, ..
            }
            | Action::SpawnCli {
                timestamp_ms: ts, ..
            }
            | Action::Wait {
                timestamp_ms: ts, ..
            }
            | Action::AssertScreenshot {
                timestamp_ms: ts, ..
            } => {
                *ts = Some(timestamp_ms);
            }
        }

        self.actions.lock().unwrap().push(action.clone());

        // Flush to file if path is set
        if let Some(ref path) = *self.output_path.lock().unwrap() {
            let file = File::options().create(true).append(true).open(path)?;
            let mut writer = BufWriter::new(file);
            let line = serde_json::to_string(&action)?;
            writeln!(writer, "{}", line)?;
            writer.flush()?;
        }

        Ok(())
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.lock().unwrap()
    }

    pub fn actions(&self) -> Vec<Action> {
        self.actions.lock().unwrap().clone()
    }
}

impl Default for ActionRecorder {
    fn default() -> Self {
        Self::new()
    }
}
