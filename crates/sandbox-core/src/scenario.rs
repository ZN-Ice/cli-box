use crate::error::Result;
use crate::player::{ActionPlayer, ActionResult};
use crate::recorder::Action;
use crate::report::{StepResult, StepStatus, TestReport};
use serde::Deserialize;
use std::path::Path;
use std::time::Instant;

/// A test scenario loaded from YAML
#[derive(Debug, Deserialize)]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub steps: Vec<ScenarioStep>,
}

/// A single step in a test scenario
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScenarioStep {
    Click {
        x: f64,
        y: f64,
        #[serde(default = "default_button")]
        button: String,
    },
    DoubleClick {
        x: f64,
        y: f64,
    },
    TypeText {
        text: String,
    },
    PressKey {
        key: String,
        #[serde(default)]
        modifiers: Vec<String>,
    },
    Scroll {
        x: f64,
        y: f64,
        direction: String,
        amount: i32,
    },
    Drag {
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
    },
    Wait {
        /// Wait time in milliseconds
        duration_ms: u64,
    },
    Screenshot {
        #[serde(default)]
        label: Option<String>,
    },
    SpawnApp {
        path: String,
    },
    SpawnCli {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    AssertScreenshotDiff {
        #[serde(default)]
        label: Option<String>,
        #[serde(default = "default_threshold")]
        max_diff_percentage: f64,
    },
}

fn default_button() -> String {
    "left".to_string()
}

fn default_threshold() -> f64 {
    0.05
}

impl ScenarioStep {
    /// Convert a ScenarioStep into an Action for the player
    fn to_action(&self) -> Action {
        match self {
            ScenarioStep::Click { x, y, button } => Action::Click {
                x: *x,
                y: *y,
                button: button.clone(),
                timestamp_ms: None,
            },
            ScenarioStep::DoubleClick { x, y } => Action::DoubleClick {
                x: *x,
                y: *y,
                timestamp_ms: None,
            },
            ScenarioStep::TypeText { text } => Action::TypeText {
                text: text.clone(),
                timestamp_ms: None,
            },
            ScenarioStep::PressKey { key, modifiers } => Action::PressKey {
                key: key.clone(),
                modifiers: modifiers.clone(),
                timestamp_ms: None,
            },
            ScenarioStep::Scroll {
                x,
                y,
                direction,
                amount,
            } => Action::Scroll {
                x: *x,
                y: *y,
                direction: direction.clone(),
                amount: *amount,
                timestamp_ms: None,
            },
            ScenarioStep::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
            } => Action::Drag {
                from_x: *from_x,
                from_y: *from_y,
                to_x: *to_x,
                to_y: *to_y,
                timestamp_ms: None,
            },
            ScenarioStep::Wait { duration_ms } => Action::Wait {
                duration_ms: *duration_ms,
                timestamp_ms: None,
            },
            ScenarioStep::Screenshot { label } => Action::Screenshot {
                label: label.clone(),
                timestamp_ms: None,
            },
            ScenarioStep::SpawnApp { path } => Action::SpawnApp {
                path: path.clone(),
                timestamp_ms: None,
            },
            ScenarioStep::SpawnCli { command, args } => Action::SpawnCli {
                command: command.clone(),
                args: args.clone(),
                timestamp_ms: None,
            },
            ScenarioStep::AssertScreenshotDiff {
                label,
                max_diff_percentage,
            } => Action::AssertScreenshot {
                label: label.clone(),
                max_diff_percentage: *max_diff_percentage,
                timestamp_ms: None,
            },
        }
    }

    /// Human-readable description for the report
    fn describe(&self) -> String {
        match self {
            ScenarioStep::Click { x, y, button } => {
                format!("Click ({x}, {y}) button={button}")
            }
            ScenarioStep::DoubleClick { x, y } => format!("Double-click ({x}, {y})"),
            ScenarioStep::TypeText { text } => format!("Type: {text}"),
            ScenarioStep::PressKey { key, modifiers } => {
                format!("Press key: {key} {modifiers:?}")
            }
            ScenarioStep::Scroll {
                x,
                y,
                direction,
                amount,
            } => format!("Scroll ({x}, {y}) {direction} {amount}"),
            ScenarioStep::Drag {
                from_x,
                from_y,
                to_x,
                to_y,
            } => format!("Drag ({from_x}, {from_y}) -> ({to_x}, {to_y})"),
            ScenarioStep::Wait { duration_ms } => format!("Wait {duration_ms}ms"),
            ScenarioStep::Screenshot { label } => {
                format!(
                    "Screenshot{}",
                    label
                        .as_deref()
                        .map_or(String::new(), |l| format!(" ({l})"))
                )
            }
            ScenarioStep::SpawnApp { path } => format!("Spawn app: {path}"),
            ScenarioStep::SpawnCli { command, args } => {
                format!("Spawn CLI: {command} {args:?}")
            }
            ScenarioStep::AssertScreenshotDiff {
                label,
                max_diff_percentage,
            } => format!(
                "Assert screenshot diff (threshold: {:.2}%){}",
                max_diff_percentage * 100.0,
                label
                    .as_deref()
                    .map_or(String::new(), |l| format!(", label: {l}"))
            ),
        }
    }
}

/// Run a test scenario and produce a report
pub struct ScenarioRunner;

impl ScenarioRunner {
    /// Load a scenario from a YAML file
    pub fn load_from_file(path: &Path) -> Result<Scenario> {
        let content = std::fs::read_to_string(path)?;
        let scenario: Scenario = serde_yaml::from_str(&content).map_err(|e| {
            crate::error::AppError::Screenshot(format!("Failed to parse scenario: {e}"))
        })?;
        Ok(scenario)
    }

    /// Load a scenario from a YAML string
    pub fn load_from_str(yaml: &str) -> Result<Scenario> {
        let scenario: Scenario = serde_yaml::from_str(yaml).map_err(|e| {
            crate::error::AppError::Screenshot(format!("Failed to parse scenario: {e}"))
        })?;
        Ok(scenario)
    }

    /// Run a scenario and return a test report
    #[cfg(target_os = "macos")]
    pub async fn run(scenario: &Scenario, speed: f64) -> TestReport {
        let mut report = TestReport::new(&scenario.name);
        let mut player = ActionPlayer::new(speed);

        let actions: Vec<Action> = scenario.steps.iter().map(|s| s.to_action()).collect();

        let _start = Instant::now();
        let results = player.play(&actions).await;

        for (i, (step, result)) in scenario.steps.iter().zip(results.iter()).enumerate() {
            let step_start = Instant::now();
            let duration_ms = step_start.elapsed().as_millis() as u64;

            let step_result = match result {
                ActionResult::Ok => StepResult {
                    index: i,
                    description: step.describe(),
                    status: StepStatus::Pass,
                    duration_ms,
                    screenshot_label: None,
                    error: None,
                    diff_percentage: None,
                },
                ActionResult::Screenshot { label, .. } => StepResult {
                    index: i,
                    description: step.describe(),
                    status: StepStatus::Pass,
                    duration_ms,
                    screenshot_label: Some(label.clone()),
                    error: None,
                    diff_percentage: None,
                },
                ActionResult::DiffResult { label, diff } => {
                    let status = if diff.identical {
                        StepStatus::Pass
                    } else {
                        StepStatus::Fail
                    };
                    StepResult {
                        index: i,
                        description: step.describe(),
                        status,
                        duration_ms,
                        screenshot_label: Some(label.clone()),
                        error: if diff.identical {
                            None
                        } else {
                            Some(format!(
                                "Screenshot diff: {:.2}% ({} pixels changed)",
                                diff.diff_percentage, diff.changed_pixels
                            ))
                        },
                        diff_percentage: Some(diff.diff_percentage),
                    }
                }
                ActionResult::Error { message } => StepResult {
                    index: i,
                    description: step.describe(),
                    status: StepStatus::Fail,
                    duration_ms,
                    screenshot_label: None,
                    error: Some(message.clone()),
                    diff_percentage: None,
                },
            };

            report.add_step(step_result);
        }

        report
    }

    #[cfg(not(target_os = "macos"))]
    pub async fn run(_scenario: &Scenario, _speed: f64) -> TestReport {
        let mut report = TestReport::new(&_scenario.name);
        report.add_step(StepResult {
            index: 0,
            description: "Scenario execution".to_string(),
            status: StepStatus::Skip,
            duration_ms: 0,
            screenshot_label: None,
            error: Some("Scenario execution only available on macOS".to_string()),
            diff_percentage: None,
        });
        report
    }
}
