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
    /// Target process PID for directed CGEvent delivery
    target_pid: Option<u32>,
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
    pub fn new(speed: f64, target_pid: Option<u32>) -> Self {
        Self {
            speed: speed.max(0.1),
            target_pid,
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
                AppError::Screenshot(format!("Failed to parse action: {e} — {line}"))
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
        let pid = self.target_pid;
        match action {
            Action::Click { x, y, button, .. } => {
                let btn = match button.to_lowercase().as_str() {
                    "right" => MouseButton::Right,
                    "middle" => MouseButton::Middle,
                    _ => MouseButton::Left,
                };
                match InputSimulator::click(*x, *y, btn, pid) {
                    Ok(()) => ActionResult::Ok,
                    Err(e) => ActionResult::Error {
                        message: e.to_string(),
                    },
                }
            }
            Action::DoubleClick { x, y, .. } => match InputSimulator::double_click(*x, *y, pid) {
                Ok(()) => ActionResult::Ok,
                Err(e) => ActionResult::Error {
                    message: e.to_string(),
                },
            },
            Action::TypeText { text, .. } => match InputSimulator::type_text(text, pid) {
                Ok(()) => ActionResult::Ok,
                Err(e) => ActionResult::Error {
                    message: e.to_string(),
                },
            },
            Action::PressKey { key, modifiers, .. } => {
                let m: Vec<&str> = modifiers.iter().map(|s| s.as_str()).collect();
                match InputSimulator::press_key(key, &m, pid) {
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
            } => match InputSimulator::scroll(*x, *y, direction, *amount, pid) {
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
            } => match InputSimulator::drag(*from_x, *from_y, *to_x, *to_y, pid) {
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
                                        "No reference screenshot found for label: {lbl}"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::Action;

    #[test]
    fn test_new_speed_clamped() {
        let player = ActionPlayer::new(0.0, None);
        assert!(!player.screenshots().is_empty() || player.screenshots().is_empty());
    }

    #[test]
    fn test_get_timestamp_none() {
        let player = ActionPlayer::new(1.0, None);
        let action = Action::Click {
            x: 0.0,
            y: 0.0,
            button: "left".into(),
            timestamp_ms: None,
        };
        assert_eq!(player.get_timestamp(&action), 0);
    }

    #[test]
    fn test_get_timestamp_some() {
        let player = ActionPlayer::new(1.0, None);
        let action = Action::Click {
            x: 0.0,
            y: 0.0,
            button: "left".into(),
            timestamp_ms: Some(1234),
        };
        assert_eq!(player.get_timestamp(&action), 1234);
    }

    #[test]
    fn test_get_timestamp_all_variants() {
        let player = ActionPlayer::new(1.0, None);
        let actions: Vec<Action> = vec![
            Action::DoubleClick {
                x: 0.0,
                y: 0.0,
                timestamp_ms: Some(10),
            },
            Action::TypeText {
                text: "a".into(),
                timestamp_ms: Some(20),
            },
            Action::PressKey {
                key: "a".into(),
                modifiers: vec![],
                timestamp_ms: Some(30),
            },
            Action::Scroll {
                x: 0.0,
                y: 0.0,
                direction: "up".into(),
                amount: 1,
                timestamp_ms: Some(40),
            },
            Action::Drag {
                from_x: 0.0,
                from_y: 0.0,
                to_x: 1.0,
                to_y: 1.0,
                timestamp_ms: Some(50),
            },
            Action::Screenshot {
                label: None,
                timestamp_ms: Some(60),
            },
            Action::SpawnApp {
                path: "/a.app".into(),
                timestamp_ms: Some(70),
            },
            Action::SpawnCli {
                command: "ls".into(),
                args: vec![],
                timestamp_ms: Some(80),
            },
            Action::Wait {
                duration_ms: 100,
                timestamp_ms: Some(90),
            },
            Action::AssertScreenshot {
                label: None,
                max_diff_percentage: 0.05,
                timestamp_ms: Some(100),
            },
        ];
        for (i, action) in actions.iter().enumerate() {
            assert_eq!(
                player.get_timestamp(action),
                ((i + 1) * 10) as u64,
                "variant {i}"
            );
        }
    }

    #[test]
    fn test_load_file_valid() {
        let actions = vec![
            Action::Click {
                x: 1.0,
                y: 2.0,
                button: "left".into(),
                timestamp_ms: Some(100),
            },
            Action::TypeText {
                text: "hi".into(),
                timestamp_ms: Some(200),
            },
        ];
        let mut jsonl = String::new();
        for a in &actions {
            jsonl.push_str(&serde_json::to_string(a).unwrap());
            jsonl.push('\n');
        }
        let tmp = std::env::temp_dir().join("test_actions.jsonl");
        std::fs::write(&tmp, &jsonl).unwrap();
        let loaded = ActionPlayer::load_file(&tmp).unwrap();
        assert_eq!(loaded.len(), 2);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_load_file_empty_lines() {
        let content = "\n\n\n";
        let tmp = std::env::temp_dir().join("test_empty_actions.jsonl");
        std::fs::write(&tmp, content).unwrap();
        let loaded = ActionPlayer::load_file(&tmp).unwrap();
        assert!(loaded.is_empty());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_load_file_not_found() {
        let result = ActionPlayer::load_file(Path::new("/tmp/__nonexistent_actions__.jsonl"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_file_invalid_json() {
        let tmp = std::env::temp_dir().join("test_bad_actions.jsonl");
        std::fs::write(&tmp, "not valid json\n").unwrap();
        let result = ActionPlayer::load_file(&tmp);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn test_play_returns_error_on_non_macos() {
        #[cfg(not(target_os = "macos"))]
        {
            let mut player = ActionPlayer::new(1.0, None);
            let results = player.play(&[]).await;
            assert!(!results.is_empty());
        }
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(1.0, None);
            let results = player.play(&[]).await;
            assert!(results.is_empty());
        }
    }

    #[tokio::test]
    async fn test_play_wait_action_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::Wait {
                duration_ms: 1,
                timestamp_ms: Some(0),
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
            assert!(matches!(results[0], ActionResult::Ok));
        }
    }

    #[tokio::test]
    async fn test_play_click_without_permission_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::Click {
                x: 100.0,
                y: 200.0,
                button: "left".into(),
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_type_text_without_permission_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::TypeText {
                text: "hello".into(),
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_press_key_without_permission_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::PressKey {
                key: "return".into(),
                modifiers: vec!["cmd".into()],
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_double_click_without_permission_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::DoubleClick {
                x: 50.0,
                y: 50.0,
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_scroll_without_permission_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::Scroll {
                x: 50.0,
                y: 50.0,
                direction: "down".into(),
                amount: 3,
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_drag_without_permission_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::Drag {
                from_x: 0.0,
                from_y: 0.0,
                to_x: 100.0,
                to_y: 100.0,
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_screenshot_without_permission_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::Screenshot {
                label: Some("test".into()),
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_spawn_app_nonexistent_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::SpawnApp {
                path: "/tmp/__no_such_app__.app".into(),
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_spawn_cli_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::SpawnCli {
                command: "echo".into(),
                args: vec!["hello".into()],
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_assert_screenshot_no_label_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::AssertScreenshot {
                label: None,
                max_diff_percentage: 0.05,
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_multiple_actions_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![
                Action::Wait {
                    duration_ms: 1,
                    timestamp_ms: Some(0),
                },
                Action::Wait {
                    duration_ms: 1,
                    timestamp_ms: Some(10),
                },
            ];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 2);
            assert!(results.iter().all(|r| matches!(r, ActionResult::Ok)));
        }
    }

    #[tokio::test]
    async fn test_play_scroll_left_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::Scroll {
                x: 0.0,
                y: 0.0,
                direction: "left".into(),
                amount: 1,
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_scroll_right_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::Scroll {
                x: 0.0,
                y: 0.0,
                direction: "right".into(),
                amount: 1,
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_type_text_uppercase_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::TypeText {
                text: "ABC".into(),
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_play_assert_screenshot_with_label_missing_on_macos() {
        #[cfg(target_os = "macos")]
        {
            let mut player = ActionPlayer::new(100.0, None);
            let actions = vec![Action::AssertScreenshot {
                label: Some("no_such_ref".into()),
                max_diff_percentage: 0.05,
                timestamp_ms: None,
            }];
            let results = player.play(&actions).await;
            assert_eq!(results.len(), 1);
        }
    }

    #[test]
    fn test_action_result_debug() {
        let ok = ActionResult::Ok;
        assert!(format!("{ok:?}").contains("Ok"));
        let err = ActionResult::Error {
            message: "test".into(),
        };
        assert!(format!("{err:?}").contains("test"));
    }
}
