use sandbox_core::sandbox::{Sandbox, SandboxConfig, SandboxState, SubWindow};

#[test]
fn sandbox_config_default_values() {
    let config = SandboxConfig::default();
    assert_eq!(config.width, 1280);
    assert_eq!(config.height, 800);
    assert_eq!(config.title, "CLI Box");
    assert!(config.id.is_none());
    assert!(config.port.is_none());
    assert!(config.mode.is_none());
    assert!(config.command.is_none());
    assert!(config.args.is_empty());
}

#[test]
fn sandbox_config_full_custom() {
    let config = SandboxConfig {
        id: Some("sandbox-01".into()),
        port: Some(9999),
        mode: Some("app".into()),
        command: Some("/Applications/Test.app".into()),
        args: vec!["--verbose".into()],
        width: 1920,
        height: 1080,
        title: "Custom Title".into(),
    };
    let sandbox = Sandbox::new(config.clone());
    assert_eq!(sandbox.config().width, 1920);
    assert_eq!(sandbox.config().height, 1080);
    assert_eq!(sandbox.id(), Some("sandbox-01"));
    assert_eq!(sandbox.port(), Some(9999));
}

#[test]
fn sandbox_state_default() {
    let sandbox = Sandbox::new(SandboxConfig::default());
    let state = sandbox.state();
    assert!(!state.is_running);
    assert!(state.window_id.is_none());
    assert!(state.sandbox_id.is_none());
    assert!(state.port.is_none());
    assert!(state.sub_windows.is_empty());
    assert_eq!(state.width, 1280);
    assert_eq!(state.height, 800);
}

#[test]
fn sandbox_state_after_init() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(99).unwrap();
    let state = sandbox.state();
    assert!(state.is_running);
    assert_eq!(state.window_id, Some(99));
}

#[test]
fn sub_window_creation() {
    let w = SubWindow {
        id: 100,
        title: "Test Window".into(),
    };
    let json = serde_json::to_string(&w).unwrap();
    assert!(json.contains("100"));
    assert!(json.contains("Test Window"));

    let parsed: SubWindow = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, 100);
    assert_eq!(parsed.title, "Test Window");
}

#[test]
fn sub_window_empty_title() {
    let w = SubWindow {
        id: 1,
        title: String::new(),
    };
    let json = serde_json::to_string(&w).unwrap();
    let parsed: SubWindow = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.title, "");
}

#[test]
fn sandbox_state_serialization() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(42).unwrap();
    sandbox.add_window(100, "Sub".into());

    let json = serde_json::to_string(sandbox.state()).unwrap();
    assert!(json.contains("\"window_id\":42"));
    assert!(json.contains("\"is_running\":true"));
    assert!(json.contains("\"id\":100"));
    assert!(json.contains("\"title\":\"Sub\""));

    let state: SandboxState = serde_json::from_str(&json).unwrap();
    assert_eq!(state.window_id, Some(42));
    assert!(state.is_running);
    assert_eq!(state.sub_windows.len(), 1);
    assert_eq!(state.sub_windows[0].id, 100);
}

#[test]
fn sandbox_list_windows_includes_main() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(1).unwrap();
    sandbox.add_window(10, "Sub A".into());
    sandbox.add_window(20, "Sub B".into());

    let windows = sandbox.list_windows();
    assert_eq!(windows.len(), 3);
    assert_eq!(windows[0].id, 1); // main window first
    assert_eq!(windows[0].title, "CLI Box");
    assert_eq!(windows[1].id, 10);
    assert_eq!(windows[2].id, 20);
}

#[test]
fn sandbox_remove_nonexistent_window_noop() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(1).unwrap();
    sandbox.remove_window(999);
    assert_eq!(sandbox.list_windows().len(), 1);
}

#[test]
fn sandbox_shutdown_clears_all() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(42).unwrap();
    sandbox.add_window(10, "Sub".into());
    sandbox.shutdown();

    assert!(!sandbox.state().is_running);
    assert!(sandbox.state().window_id.is_none());
    assert!(sandbox.state().sub_windows.is_empty());
}

#[test]
fn sandbox_config_serialization_roundtrip() {
    let config = SandboxConfig {
        id: Some("abc".into()),
        port: Some(12345),
        mode: Some("cli".into()),
        command: Some("claude".into()),
        args: vec!["--help".into()],
        width: 1440,
        height: 900,
        title: "Test".into(),
    };
    let json = serde_json::to_string(&config).unwrap();
    let parsed: SandboxConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, Some("abc".into()));
    assert_eq!(parsed.port, Some(12345));
    assert_eq!(parsed.mode, Some("cli".into()));
    assert_eq!(parsed.command, Some("claude".into()));
    assert_eq!(parsed.args, vec!["--help"]);
    assert_eq!(parsed.width, 1440);
    assert_eq!(parsed.height, 900);
}

#[test]
fn sandbox_kind_cli() {
    let config = SandboxConfig {
        mode: Some("cli".into()),
        command: Some("vim".into()),
        args: vec!["file.txt".into()],
        ..SandboxConfig::default()
    };
    let sandbox = Sandbox::new(config);
    let kind = sandbox.kind().unwrap();
    match kind {
        sandbox_core::instance::InstanceKind::Cli { command, args } => {
            assert_eq!(command, "vim");
            assert_eq!(args, vec!["file.txt"]);
        }
        other => panic!("Expected Cli kind, got: {other:?}"),
    }
}

#[test]
fn sandbox_kind_app() {
    let config = SandboxConfig {
        mode: Some("app".into()),
        command: Some("/Applications/Safari.app".into()),
        ..SandboxConfig::default()
    };
    let sandbox = Sandbox::new(config);
    let kind = sandbox.kind().unwrap();
    match kind {
        sandbox_core::instance::InstanceKind::App { path } => {
            assert_eq!(path, "/Applications/Safari.app");
        }
        other => panic!("Expected App kind, got: {other:?}"),
    }
}

#[test]
fn sandbox_kind_unknown_mode_returns_none() {
    let config = SandboxConfig {
        mode: Some("unknown".into()),
        command: Some("something".into()),
        ..SandboxConfig::default()
    };
    let sandbox = Sandbox::new(config);
    assert!(sandbox.kind().is_none());
}

#[test]
fn sandbox_kind_missing_command_returns_none() {
    let config = SandboxConfig {
        mode: Some("cli".into()),
        command: None,
        ..SandboxConfig::default()
    };
    let sandbox = Sandbox::new(config);
    assert!(sandbox.kind().is_none());
}

#[test]
fn sandbox_uptime_zero_before_init() {
    let sandbox = Sandbox::new(SandboxConfig::default());
    assert_eq!(sandbox.uptime_secs(), 0);
}

#[test]
fn sandbox_uptime_increases_after_init() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(1).unwrap();
    // Uptime should be >= 0 (could be 0 if init runs fast enough)
    let uptime = sandbox.uptime_secs();
    assert!(
        uptime < 5,
        "uptime should be small just after init, got {uptime}"
    );
}

#[test]
fn sandbox_multiple_add_remove_windows() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(1).unwrap();

    for i in 0..100 {
        sandbox.add_window(i + 100, format!("Window-{i}"));
    }
    assert_eq!(sandbox.list_windows().len(), 101); // main + 100 subs

    for i in 0..50 {
        sandbox.remove_window(i + 100);
    }
    assert_eq!(sandbox.list_windows().len(), 51); // main + 50 remaining
}

#[test]
fn sandbox_set_window_id_updates_state() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.set_window_id(77);
    assert_eq!(sandbox.window_id(), Some(77));
    assert!(sandbox.state().is_running);
}
