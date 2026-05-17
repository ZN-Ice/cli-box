use sandbox_core::report::{StepResult, StepStatus, TestReport};
use sandbox_core::scenario::{ScenarioRunner, ScenarioStep};

const FULL_SCENARIO: &str = r#"
name: "Full feature test"
description: "Tests all 12 action types"
steps:
  - type: click
    x: 100
    y: 200
    button: left
  - type: double_click
    x: 150
    y: 250
  - type: type_text
    text: "hello world"
  - type: press_key
    key: return
    modifiers:
      - cmd
  - type: scroll
    x: 100
    y: 200
    direction: down
    amount: 3
  - type: drag
    from_x: 100
    from_y: 100
    to_x: 200
    to_y: 200
  - type: wait
    duration_ms: 500
  - type: screenshot
    label: "before_action"
  - type: spawn_app
    path: "/Applications/Calculator.app"
  - type: spawn_cli
    command: "echo"
    args:
      - "hello"
  - type: assert_screenshot_diff
    label: "before_action"
    max_diff_percentage: 0.05
"#;

#[test]
fn load_scenario_from_yaml() {
    let scenario = ScenarioRunner::load_from_str(FULL_SCENARIO).unwrap();
    assert_eq!(scenario.name, "Full feature test");
    assert_eq!(
        scenario.description.as_deref(),
        Some("Tests all 12 action types")
    );
    assert_eq!(scenario.steps.len(), 11);
}

#[test]
fn parse_all_step_types() {
    let scenario = ScenarioRunner::load_from_str(FULL_SCENARIO).unwrap();

    // Verify each step parsed correctly
    match &scenario.steps[0] {
        ScenarioStep::Click { x, y, button } => {
            assert_eq!(*x, 100.0);
            assert_eq!(*y, 200.0);
            assert_eq!(button, "left");
        }
        _ => panic!("expected Click"),
    }

    match &scenario.steps[1] {
        ScenarioStep::DoubleClick { x, y } => {
            assert_eq!(*x, 150.0);
            assert_eq!(*y, 250.0);
        }
        _ => panic!("expected DoubleClick"),
    }

    match &scenario.steps[2] {
        ScenarioStep::TypeText { text } => assert_eq!(text, "hello world"),
        _ => panic!("expected TypeText"),
    }

    match &scenario.steps[3] {
        ScenarioStep::PressKey { key, modifiers } => {
            assert_eq!(key, "return");
            assert_eq!(modifiers, &["cmd"]);
        }
        _ => panic!("expected PressKey"),
    }

    match &scenario.steps[4] {
        ScenarioStep::Scroll {
            x,
            y,
            direction,
            amount,
        } => {
            assert_eq!(*x, 100.0);
            assert_eq!(*y, 200.0);
            assert_eq!(direction, "down");
            assert_eq!(*amount, 3);
        }
        _ => panic!("expected Scroll"),
    }

    match &scenario.steps[5] {
        ScenarioStep::Drag {
            from_x,
            from_y,
            to_x,
            to_y,
        } => {
            assert_eq!(*from_x, 100.0);
            assert_eq!(*from_y, 100.0);
            assert_eq!(*to_x, 200.0);
            assert_eq!(*to_y, 200.0);
        }
        _ => panic!("expected Drag"),
    }

    match &scenario.steps[6] {
        ScenarioStep::Wait { duration_ms } => assert_eq!(*duration_ms, 500),
        _ => panic!("expected Wait"),
    }

    match &scenario.steps[7] {
        ScenarioStep::Screenshot { label } => assert_eq!(label.as_deref(), Some("before_action")),
        _ => panic!("expected Screenshot"),
    }

    match &scenario.steps[8] {
        ScenarioStep::SpawnApp { path } => assert!(path.contains("Calculator.app")),
        _ => panic!("expected SpawnApp"),
    }

    match &scenario.steps[9] {
        ScenarioStep::SpawnCli { command, args } => {
            assert_eq!(command, "echo");
            assert_eq!(args, &["hello"]);
        }
        _ => panic!("expected SpawnCli"),
    }

    match &scenario.steps[10] {
        ScenarioStep::AssertScreenshotDiff {
            label,
            max_diff_percentage,
        } => {
            assert_eq!(label.as_deref(), Some("before_action"));
            assert!((*max_diff_percentage - 0.05).abs() < 0.001);
        }
        _ => panic!("expected AssertScreenshotDiff"),
    }
}

// ── TestReport ───────────────────────────────────────────────

#[test]
fn empty_report_passes() {
    let report = TestReport::new("empty test");
    assert_eq!(report.name, "empty test");
    assert_eq!(report.status, StepStatus::Pass);
    assert_eq!(report.total_steps, 0);
    assert_eq!(report.passed_steps, 0);
    assert_eq!(report.failed_steps, 0);
}

#[test]
fn report_tracks_pass_fail_counts() {
    let mut report = TestReport::new("mixed test");

    report.add_step(StepResult {
        index: 0,
        description: "step 1".into(),
        status: StepStatus::Pass,
        duration_ms: 100,
        screenshot_label: None,
        error: None,
        diff_percentage: None,
    });

    report.add_step(StepResult {
        index: 1,
        description: "step 2".into(),
        status: StepStatus::Fail,
        duration_ms: 200,
        screenshot_label: None,
        error: Some("something broke".into()),
        diff_percentage: None,
    });

    report.add_step(StepResult {
        index: 2,
        description: "step 3".into(),
        status: StepStatus::Pass,
        duration_ms: 50,
        screenshot_label: Some("s1".into()),
        error: None,
        diff_percentage: None,
    });

    assert_eq!(report.status, StepStatus::Fail);
    assert_eq!(report.total_steps, 3);
    assert_eq!(report.passed_steps, 2);
    assert_eq!(report.failed_steps, 1);
    assert_eq!(report.duration_ms, 350);
}

#[test]
fn report_renders_markdown() {
    let mut report = TestReport::new("smoke test");
    report.add_step(StepResult {
        index: 0,
        description: "click button".into(),
        status: StepStatus::Pass,
        duration_ms: 123,
        screenshot_label: None,
        error: None,
        diff_percentage: None,
    });

    let md = report.to_markdown();
    assert!(md.contains("smoke test"));
    assert!(md.contains("click button"));
    assert!(md.contains("PASSED"));
    assert!(md.contains("123ms"));
}

#[test]
fn report_renders_failure_markdown() {
    let mut report = TestReport::new("failing test");
    report.add_step(StepResult {
        index: 0,
        description: "bad step".into(),
        status: StepStatus::Fail,
        duration_ms: 10,
        screenshot_label: None,
        error: Some("crash".into()),
        diff_percentage: None,
    });

    let md = report.to_markdown();
    assert!(md.contains("FAILED"));
    assert!(md.contains("crash"));
}

#[test]
fn report_renders_json() {
    let mut report = TestReport::new("json test");
    report.add_step(StepResult {
        index: 0,
        description: "step".into(),
        status: StepStatus::Pass,
        duration_ms: 5,
        screenshot_label: None,
        error: None,
        diff_percentage: None,
    });

    let json = report.to_json().unwrap();
    assert!(json.contains("json test"));
    assert!(json.contains("step"));
}

#[test]
fn report_renders_html() {
    let mut report = TestReport::new("html test");
    report.add_step(StepResult {
        index: 0,
        description: "step".into(),
        status: StepStatus::Pass,
        duration_ms: 5,
        screenshot_label: None,
        error: None,
        diff_percentage: None,
    });

    let html = report.to_html();
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("html test"));
    assert!(html.contains("step"));
    assert!(html.contains("</html>"));
}

#[test]
fn report_with_diff_shows_percentage() {
    let mut report = TestReport::new("diff report");
    report.add_step(StepResult {
        index: 0,
        description: "compare screenshots".into(),
        status: StepStatus::Fail,
        duration_ms: 10,
        screenshot_label: Some("ref".into()),
        error: None,
        diff_percentage: Some(12.34),
    });

    let md = report.to_markdown();
    assert!(md.contains("12.34%"));
    assert!(md.contains("FAILED"));
}

// ── StepStatus ───────────────────────────────────────────────

#[test]
fn step_status_serialization() {
    let pass = serde_json::to_string(&StepStatus::Pass).unwrap();
    assert_eq!(pass, r#""pass""#);

    let fail: StepStatus = serde_json::from_str(r#""fail""#).unwrap();
    assert_eq!(fail, StepStatus::Fail);

    let skip: StepStatus = serde_json::from_str(r#""skip""#).unwrap();
    assert_eq!(skip, StepStatus::Skip);
}

// ── Minimal YAML scenarios ───────────────────────────────────

#[test]
fn minimal_scenario_no_description() {
    let yaml = r#"
name: "minimal"
steps:
  - type: wait
    duration_ms: 100
"#;
    let scenario = ScenarioRunner::load_from_str(yaml).unwrap();
    assert_eq!(scenario.name, "minimal");
    assert!(scenario.description.is_none());
    assert_eq!(scenario.steps.len(), 1);
}

#[test]
fn scenario_with_default_values() {
    let yaml = r#"
name: "defaults"
steps:
  - type: click
    x: 10
    y: 20
  - type: assert_screenshot_diff
    label: "foo"
"#;
    let scenario = ScenarioRunner::load_from_str(yaml).unwrap();
    // Click defaults to button=left
    match &scenario.steps[0] {
        ScenarioStep::Click { button, .. } => assert_eq!(button, "left"),
        _ => panic!(),
    }
    // AssertScreenshotDiff defaults to threshold=0.05
    match &scenario.steps[1] {
        ScenarioStep::AssertScreenshotDiff {
            max_diff_percentage,
            ..
        } => {
            assert!((*max_diff_percentage - 0.05).abs() < 0.001);
        }
        _ => panic!(),
    }
}

#[test]
fn invalid_yaml_returns_error() {
    let yaml = "not: valid: yaml: [";
    let result = ScenarioRunner::load_from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn empty_steps_allowed() {
    let yaml = r#"
name: "no steps"
steps: []
"#;
    let scenario = ScenarioRunner::load_from_str(yaml).unwrap();
    assert!(scenario.steps.is_empty());
}

#[test]
fn report_with_skip_step_markdown() {
    let mut report = TestReport::new("skip test");
    report.add_step(StepResult {
        index: 0,
        description: "skipped step".into(),
        status: StepStatus::Skip,
        duration_ms: 0,
        screenshot_label: None,
        error: None,
        diff_percentage: None,
    });

    let md = report.to_markdown();
    assert!(md.contains("⏭️"));
    assert!(md.contains("skipped step"));
    assert!(md.contains("PASSED"));
}

#[test]
fn report_with_skip_step_html() {
    let mut report = TestReport::new("skip html");
    report.add_step(StepResult {
        index: 0,
        description: "skipped".into(),
        status: StepStatus::Skip,
        duration_ms: 0,
        screenshot_label: None,
        error: None,
        diff_percentage: None,
    });

    let html = report.to_html();
    assert!(html.contains(r#"class="skip""#));
    assert!(html.contains("SKIP"));
    assert!(html.contains("</html>"));
}

#[test]
fn report_with_mixed_status_html() {
    let mut report = TestReport::new("mixed html");
    report.add_step(StepResult {
        index: 0,
        description: "ok step".into(),
        status: StepStatus::Pass,
        duration_ms: 10,
        screenshot_label: None,
        error: None,
        diff_percentage: None,
    });
    report.add_step(StepResult {
        index: 1,
        description: "bad step".into(),
        status: StepStatus::Fail,
        duration_ms: 5,
        screenshot_label: None,
        error: Some("error msg".into()),
        diff_percentage: None,
    });
    report.add_step(StepResult {
        index: 2,
        description: "skip step".into(),
        status: StepStatus::Skip,
        duration_ms: 0,
        screenshot_label: None,
        error: None,
        diff_percentage: None,
    });

    let html = report.to_html();
    assert!(html.contains("FAIL"));
    assert!(html.contains("PASS"));
    assert!(html.contains("SKIP"));
    assert!(html.contains("error msg"));
}
