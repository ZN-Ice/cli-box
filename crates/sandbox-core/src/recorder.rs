use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

/// A recorded action with timing information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedAction {
    /// Milliseconds since recording started
    pub offset_ms: u64,
    /// The action type
    pub action: ActionType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActionType {
    #[serde(rename = "type")]
    Type { text: String, pty: bool },
    #[serde(rename = "key")]
    Key {
        key: String,
        modifiers: Vec<String>,
        pty: bool,
    },
    #[serde(rename = "click")]
    Click { x: f64, y: f64, button: String },
    #[serde(rename = "screenshot")]
    Screenshot { path: String },
    #[serde(rename = "wait")]
    Wait { ms: u64 },
}

/// Records actions to a JSONL file.
pub struct Recorder {
    writer: BufWriter<File>,
    start: Instant,
}

impl Recorder {
    pub fn start(path: &PathBuf) -> Result<Self> {
        let file = File::create(path)?;
        Ok(Self {
            writer: BufWriter::new(file),
            start: Instant::now(),
        })
    }

    pub fn record(&mut self, action: ActionType) -> Result<()> {
        let offset_ms = self.start.elapsed().as_millis() as u64;
        let entry = RecordedAction { offset_ms, action };
        serde_json::to_writer(&mut self.writer, &entry)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }
}
