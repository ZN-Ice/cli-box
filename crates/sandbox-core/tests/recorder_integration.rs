use sandbox_core::recorder::{Action, ActionRecorder};

#[test]
fn recorder_starts_disabled() {
    let recorder = ActionRecorder::new();
    assert!(!recorder.is_enabled());
    assert!(recorder.actions().is_empty());
}

#[test]
fn start_enables_and_clears() {
    let recorder = ActionRecorder::new();
    recorder.start(None).unwrap();
    assert!(recorder.is_enabled());
    assert!(recorder.actions().is_empty());
}

#[test]
fn record_while_disabled_is_ignored() {
    let recorder = ActionRecorder::new();
    recorder
        .record(Action::Wait {
            duration_ms: 100,
            timestamp_ms: None,
        })
        .unwrap();
    assert!(recorder.actions().is_empty());
}

#[test]
fn record_while_enabled_captures() {
    let recorder = ActionRecorder::new();
    recorder.start(None).unwrap();
    recorder
        .record(Action::Click {
            x: 10.0,
            y: 20.0,
            button: "left".into(),
            timestamp_ms: None,
        })
        .unwrap();

    let actions = recorder.actions();
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        Action::Click {
            x,
            y,
            button,
            timestamp_ms,
        } => {
            assert_eq!(*x, 10.0);
            assert_eq!(*y, 20.0);
            assert_eq!(button, "left");
            assert!(timestamp_ms.is_some()); // auto-filled
        }
        _ => panic!("expected Click"),
    }
}

#[test]
fn stop_disables_and_returns_actions() {
    let recorder = ActionRecorder::new();
    recorder.start(None).unwrap();
    recorder
        .record(Action::Wait {
            duration_ms: 100,
            timestamp_ms: None,
        })
        .unwrap();
    recorder
        .record(Action::Wait {
            duration_ms: 200,
            timestamp_ms: None,
        })
        .unwrap();

    let actions = recorder.stop().unwrap();
    assert_eq!(actions.len(), 2);
    assert!(!recorder.is_enabled());
}

#[test]
fn timestamps_are_monotonic() {
    let recorder = ActionRecorder::new();
    recorder.start(None).unwrap();
    recorder
        .record(Action::Wait {
            duration_ms: 10,
            timestamp_ms: None,
        })
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    recorder
        .record(Action::Wait {
            duration_ms: 10,
            timestamp_ms: None,
        })
        .unwrap();

    let actions = recorder.actions();
    let t0 = match &actions[0] {
        Action::Wait { timestamp_ms, .. } => timestamp_ms.unwrap(),
        _ => 0,
    };
    let t1 = match &actions[1] {
        Action::Wait { timestamp_ms, .. } => timestamp_ms.unwrap(),
        _ => 0,
    };
    assert!(t1 >= t0);
}

#[test]
fn all_action_types_record_timestamps() {
    let recorder = ActionRecorder::new();
    recorder.start(None).unwrap();

    let actions = vec![
        Action::Click {
            x: 0.0,
            y: 0.0,
            button: "left".into(),
            timestamp_ms: None,
        },
        Action::DoubleClick {
            x: 0.0,
            y: 0.0,
            timestamp_ms: None,
        },
        Action::TypeText {
            text: "hi".into(),
            timestamp_ms: None,
        },
        Action::PressKey {
            key: "return".into(),
            modifiers: vec![],
            timestamp_ms: None,
        },
        Action::Scroll {
            x: 0.0,
            y: 0.0,
            direction: "down".into(),
            amount: 1,
            timestamp_ms: None,
        },
        Action::Drag {
            from_x: 0.0,
            from_y: 0.0,
            to_x: 1.0,
            to_y: 1.0,
            timestamp_ms: None,
        },
        Action::Screenshot {
            label: Some("s".into()),
            timestamp_ms: None,
        },
        Action::SpawnApp {
            path: "/a.app".into(),
            timestamp_ms: None,
        },
        Action::SpawnCli {
            command: "ls".into(),
            args: vec![],
            timestamp_ms: None,
        },
        Action::Wait {
            duration_ms: 100,
            timestamp_ms: None,
        },
        Action::AssertScreenshot {
            label: Some("s".into()),
            max_diff_percentage: 0.05,
            timestamp_ms: None,
        },
    ];

    for a in &actions {
        recorder.record(a.clone()).unwrap();
    }

    let recorded = recorder.actions();
    assert_eq!(recorded.len(), actions.len());

    // Verify every action got a timestamp
    let check_ts = |a: &Action| {
        let ts = match a {
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
            | Action::AssertScreenshot { timestamp_ms, .. } => *timestamp_ms,
        };
        assert!(ts.is_some(), "action missing timestamp");
    };

    for a in &recorded {
        check_ts(a);
    }
}

#[test]
fn action_json_roundtrip() {
    let orig = Action::Click {
        x: 1.5,
        y: 2.5,
        button: "right".into(),
        timestamp_ms: Some(42),
    };
    let json = serde_json::to_string(&orig).unwrap();
    let parsed: Action = serde_json::from_str(&json).unwrap();
    match parsed {
        Action::Click {
            x,
            y,
            button,
            timestamp_ms,
        } => {
            assert_eq!(x, 1.5);
            assert_eq!(y, 2.5);
            assert_eq!(button, "right");
            assert_eq!(timestamp_ms, Some(42));
        }
        _ => panic!("roundtrip failed"),
    }
}

#[test]
fn action_serde_tagged_enum() {
    // Verify JSON uses "type" field
    let action = Action::TypeText {
        text: "hello".into(),
        timestamp_ms: None,
    };
    let json = serde_json::to_string(&action).unwrap();
    assert!(json.contains(r#""type":"type_text""#));
    assert!(json.contains("hello"));
}

#[test]
fn default_recorder() {
    let recorder: ActionRecorder = Default::default();
    assert!(!recorder.is_enabled());
}
