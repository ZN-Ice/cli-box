use crate::error::Result;
use crate::recorder::RecordedAction;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

/// Callback invoked for each action during playback.
pub type ActionCallback = Box<
    dyn Fn(
            RecordedAction,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Replays actions from a JSONL file.
pub struct Player;

impl Player {
    pub async fn play(path: &Path, callback: &ActionCallback) -> Result<()> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut last_offset: u64 = 0;

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let action: RecordedAction = serde_json::from_str(&line)?;

            let delay = action.offset_ms.saturating_sub(last_offset);
            if delay > 0 {
                sleep(Duration::from_millis(delay)).await;
            }
            last_offset = action.offset_ms;

            info!(
                "Playing action at {}ms: {:?}",
                action.offset_ms, action.action
            );
            callback(action).await?;
        }
        Ok(())
    }

    pub fn load_actions(path: &Path) -> Result<Vec<RecordedAction>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut actions = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            actions.push(serde_json::from_str(&line)?);
        }
        Ok(actions)
    }
}
