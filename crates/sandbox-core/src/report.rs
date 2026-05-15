use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Represents the outcome of a single test step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// Step index (0-based)
    pub index: usize,
    /// Human-readable step description
    pub description: String,
    /// Step status
    pub status: StepStatus,
    /// Duration of this step
    pub duration_ms: u64,
    /// Optional screenshot captured during this step
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_label: Option<String>,
    /// Optional error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Optional diff result for assertion steps
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_percentage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pass,
    Fail,
    Skip,
}

/// Full test report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    /// Test scenario name
    pub name: String,
    /// Overall test status
    pub status: StepStatus,
    /// Total duration
    pub duration_ms: u64,
    /// Number of steps
    pub total_steps: usize,
    /// Number of passed steps
    pub passed_steps: usize,
    /// Number of failed steps
    pub failed_steps: usize,
    /// Per-step results
    pub steps: Vec<StepResult>,
}

impl TestReport {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: StepStatus::Pass,
            duration_ms: 0,
            total_steps: 0,
            passed_steps: 0,
            failed_steps: 0,
            steps: Vec::new(),
        }
    }

    /// Add a step result and update summary
    pub fn add_step(&mut self, step: StepResult) {
        if step.status == StepStatus::Fail {
            self.status = StepStatus::Fail;
            self.failed_steps += 1;
        } else if step.status == StepStatus::Pass {
            self.passed_steps += 1;
        }
        self.total_steps += 1;
        self.duration_ms += step.duration_ms;
        self.steps.push(step);
    }

    /// Render report as Markdown
    pub fn to_markdown(&self) -> String {
        let status_icon = match self.status {
            StepStatus::Pass => "✅",
            StepStatus::Fail => "❌",
            StepStatus::Skip => "⏭️",
        };

        let mut md = format!(
            "# {} Test Report: {}\n\n\
             **Status**: {}  \n\
             **Duration**: {:.2}s  \n\
             **Steps**: {} total, {} passed, {} failed\n\n\
             | # | Description | Status | Duration | Details |\n\
             |---|------------|--------|----------|--------|\n",
            status_icon,
            self.name,
            match self.status {
                StepStatus::Pass => "PASSED",
                StepStatus::Fail => "FAILED",
                StepStatus::Skip => "SKIPPED",
            },
            self.duration_ms as f64 / 1000.0,
            self.total_steps,
            self.passed_steps,
            self.failed_steps,
        );

        for step in &self.steps {
            let icon = match step.status {
                StepStatus::Pass => "✅",
                StepStatus::Fail => "❌",
                StepStatus::Skip => "⏭️",
            };
            let details = if let Some(ref err) = step.error {
                err.clone()
            } else if let Some(diff) = step.diff_percentage {
                format!("diff: {diff:.2}%")
            } else {
                String::new()
            };
            md.push_str(&format!(
                "| {} | {} | {} | {}ms | {} |\n",
                step.index + 1,
                step.description,
                icon,
                step.duration_ms,
                details,
            ));
        }

        md
    }

    /// Render report as JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Into::into)
    }

    /// Render report as simple HTML
    pub fn to_html(&self) -> String {
        let status_class = match self.status {
            StepStatus::Pass => "pass",
            StepStatus::Fail => "fail",
            StepStatus::Skip => "skip",
        };
        let status_text = match self.status {
            StepStatus::Pass => "PASSED",
            StepStatus::Fail => "FAILED",
            StepStatus::Skip => "SKIPPED",
        };

        let mut html = format!(
            r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Test Report: {name}</title>
<style>
  body {{ font-family: system-ui; max-width: 800px; margin: 2rem auto; }}
  .header {{ text-align: center; margin-bottom: 2rem; }}
  .header .{status_class} {{ font-size: 2rem; font-weight: bold; }}
  .pass {{ color: #22c55e; }} .fail {{ color: #ef4444; }} .skip {{ color: #9ca3af; }}
  table {{ width: 100%; border-collapse: collapse; }}
  th, td {{ padding: 0.5rem; text-align: left; border-bottom: 1px solid #e5e7eb; }}
  th {{ background: #f9fafb; }}
</style></head><body>
<div class="header">
  <h1>Test Report: {name}</h1>
  <div class="{status_class}">{status_text}</div>
  <p>{passed}/{total} steps passed | {duration:.2}s</p>
</div>
<table><tr><th>#</th><th>Description</th><th>Status</th><th>Duration</th><th>Details</th></tr>"#,
            name = self.name,
            status_class = status_class,
            status_text = status_text,
            passed = self.passed_steps,
            total = self.total_steps,
            duration = self.duration_ms as f64 / 1000.0,
        );

        for step in &self.steps {
            let cls = match step.status {
                StepStatus::Pass => "pass",
                StepStatus::Fail => "fail",
                StepStatus::Skip => "skip",
            };
            let icon = match step.status {
                StepStatus::Pass => "PASS",
                StepStatus::Fail => "FAIL",
                StepStatus::Skip => "SKIP",
            };
            let diff_str = step.diff_percentage.map(|d| format!("diff: {d:.2}%"));
            let details: &str = step.error.as_deref().or(diff_str.as_deref()).unwrap_or("");
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"{}\">{}</td><td>{}ms</td><td>{}</td></tr>",
                step.index + 1,
                step.description,
                cls,
                icon,
                step.duration_ms,
                details,
            ));
        }

        html.push_str("</table></body></html>");
        html
    }
}
