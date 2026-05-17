use sandbox_core::sandbox::{Sandbox, SandboxConfig, SandboxState};

#[test]
fn new_sandbox_has_default_config() {
    let sandbox = Sandbox::new(SandboxConfig::default());
    assert_eq!(sandbox.config().width, 1280);
    assert_eq!(sandbox.config().height, 800);
    assert_eq!(sandbox.config().title, "System Test Sandbox");
}

#[test]
fn new_sandbox_is_not_initialized() {
    let sandbox = Sandbox::new(SandboxConfig::default());
    assert!(sandbox.window_id().is_none());
    assert!(!sandbox.state().is_running);
}

#[test]
fn init_sets_window_id_and_running() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(42).unwrap();
    assert_eq!(sandbox.window_id(), Some(42));
    assert!(sandbox.state().is_running);
}

#[test]
fn init_with_zero_fails() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    assert!(sandbox.init(0).is_err());
}

#[test]
fn screenshot_before_init_is_error() {
    let sandbox = Sandbox::new(SandboxConfig::default());
    assert!(sandbox.screenshot().is_err());
}

#[test]
fn screenshot_error_message_is_descriptive() {
    let sandbox = Sandbox::new(SandboxConfig::default());
    let err = sandbox.screenshot().unwrap_err();
    assert!(err.to_string().contains("Sandbox not initialized"));
}

#[test]
fn sandbox_shutdown_clears_state() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(42).unwrap();
    sandbox.shutdown();
    assert!(!sandbox.state().is_running);
    assert!(sandbox.window_id().is_none());
    assert!(sandbox.state().sub_windows.is_empty());
}

#[test]
fn set_window_id_marks_running() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.set_window_id(99);
    assert!(sandbox.state().is_running);
    assert_eq!(sandbox.window_id(), Some(99));
}

#[test]
fn sub_window_tracking() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(1).unwrap();

    sandbox.add_window(100, "App A".into());
    sandbox.add_window(200, "App B".into());

    let windows = sandbox.list_windows();
    // Main window + 2 sub-windows
    assert_eq!(windows.len(), 3);
    assert_eq!(windows[0].id, 1); // main window first
    assert_eq!(windows[0].title, "System Test Sandbox");
    assert_eq!(windows[1].id, 100);
    assert_eq!(windows[1].title, "App A");
    assert_eq!(windows[2].id, 200);
    assert_eq!(windows[2].title, "App B");

    sandbox.remove_window(100);
    let windows = sandbox.list_windows();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[1].id, 200);
}

#[test]
fn remove_nonexistent_window_is_noop() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.remove_window(999); // should not panic
    assert!(sandbox.list_windows().is_empty()); // no main window (not init)
}

#[test]
fn list_windows_without_main_shows_only_subs() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.add_window(10, "Sub".into());
    let windows = sandbox.list_windows();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].id, 10);
}

#[test]
fn custom_sandbox_config() {
    let config = SandboxConfig {
        width: 1920,
        height: 1080,
        title: "Custom Sandbox".into(),
        ..SandboxConfig::default()
    };
    let sandbox = Sandbox::new(config.clone());
    assert_eq!(sandbox.config().width, 1920);
    assert_eq!(sandbox.config().height, 1080);
    assert_eq!(sandbox.config().title, "Custom Sandbox");
}

#[test]
fn state_clone_is_independent() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(42).unwrap();

    let state: SandboxState = sandbox.state().clone();
    assert_eq!(state.window_id, Some(42));
    assert!(state.is_running);

    // Modifying sandbox doesn't affect cloned state
    sandbox.shutdown();
    assert_eq!(state.window_id, Some(42));
    assert!(state.is_running);
}

#[test]
fn multiple_init_is_idempotent() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(10).unwrap();
    sandbox.init(20).unwrap();
    assert_eq!(sandbox.window_id(), Some(20));
}

#[test]
fn sandbox_state_serialization() {
    let mut sandbox = Sandbox::new(SandboxConfig::default());
    sandbox.init(42).unwrap();

    let json = serde_json::to_string(sandbox.state()).unwrap();
    assert!(json.contains("42"));
    assert!(json.contains("true"));

    let state: SandboxState = serde_json::from_str(&json).unwrap();
    assert_eq!(state.window_id, Some(42));
    assert!(state.is_running);
}
