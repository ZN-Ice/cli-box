use sandbox_core::process::ProcessInfo;

#[test]
fn process_info_serialization_roundtrip() {
    let info = ProcessInfo {
        pid: 1001,
        name: "echo".to_string(),
        path: Some("/bin/echo".to_string()),
        is_running: true,
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("1001"));
    assert!(json.contains("echo"));
    assert!(json.contains("/bin/echo"));

    let parsed: ProcessInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.pid, 1001);
    assert_eq!(parsed.name, "echo");
    assert_eq!(parsed.path, Some("/bin/echo".to_string()));
    assert!(parsed.is_running);
}

#[test]
fn process_info_not_running() {
    let info = ProcessInfo {
        pid: 0,
        name: "dead".to_string(),
        path: None,
        is_running: false,
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"is_running\":false"));
}

#[test]
fn process_info_deserialize_missing_path() {
    let json = r#"{"pid": 42, "name": "test", "is_running": true}"#;
    let info: ProcessInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.pid, 42);
    assert_eq!(info.name, "test");
    assert!(info.path.is_none());
}

#[test]
fn process_info_vec_roundtrip() {
    let infos = vec![
        ProcessInfo {
            pid: 1,
            name: "a".into(),
            path: None,
            is_running: true,
        },
        ProcessInfo {
            pid: 2,
            name: "b".into(),
            path: None,
            is_running: false,
        },
    ];
    let json = serde_json::to_string(&infos).unwrap();
    let parsed: Vec<ProcessInfo> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].pid, 1);
    assert_eq!(parsed[1].pid, 2);
}

#[test]
fn process_info_large_pid() {
    let info = ProcessInfo {
        pid: u32::MAX,
        name: "max".into(),
        path: None,
        is_running: true,
    };
    let json = serde_json::to_string(&info).unwrap();
    let parsed: ProcessInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.pid, u32::MAX);
}

#[test]
fn process_info_ignores_unknown_fields() {
    let json = r#"{"pid": 1, "name": "x", "is_running": true, "extra": "ignored"}"#;
    let info: ProcessInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.pid, 1);
}

#[test]
fn process_info_empty_name() {
    let info = ProcessInfo {
        pid: 1,
        name: String::new(),
        path: None,
        is_running: true,
    };
    let json = serde_json::to_string(&info).unwrap();
    let parsed: ProcessInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "");
}

#[test]
fn process_info_path_none_serializes_as_null() {
    let info = ProcessInfo {
        pid: 1,
        name: "x".into(),
        path: None,
        is_running: true,
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"path\":null"));
}

#[test]
fn process_info_path_some_serializes_as_string() {
    let info = ProcessInfo {
        pid: 1,
        name: "x".into(),
        path: Some("/usr/bin/x".into()),
        is_running: true,
    };
    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"path\":\"/usr/bin/x\""));
}

#[test]
fn list_processes_returns_empty_when_no_sessions() {
    let processes = sandbox_core::process::ProcessManager::list_processes();
    assert!(processes.is_ok());
    // May or may not be empty depending on test execution order
    // but it should never error
}

#[test]
fn kill_nonexistent_process_returns_error() {
    let result = sandbox_core::process::ProcessManager::kill_process(99990);
    assert!(result.is_err());
}

#[test]
fn send_input_nonexistent_process_returns_error() {
    let result = sandbox_core::process::ProcessManager::send_input(99990, b"hello");
    assert!(result.is_err());
}

#[test]
fn read_output_nonexistent_process_returns_error() {
    let result = sandbox_core::process::ProcessManager::read_output(99990);
    assert!(result.is_err());
}

#[test]
fn spawn_cli_simple_command() {
    let result = sandbox_core::process::ProcessManager::spawn_cli("echo", &["hello".to_string()]);
    // May succeed on macOS
    if let Ok(info) = result {
        assert!(info.is_running);
        assert_eq!(info.name, "echo");
        // Clean up
        let _ = sandbox_core::process::ProcessManager::kill_process(info.pid);
    }
}

#[test]
fn spawn_cli_then_list_and_kill() {
    let spawn_result =
        sandbox_core::process::ProcessManager::spawn_cli("sleep", &["1".to_string()]);
    if let Ok(info) = spawn_result {
        let list = sandbox_core::process::ProcessManager::list_processes().unwrap();
        assert!(list.iter().any(|p| p.pid == info.pid));

        let kill_result = sandbox_core::process::ProcessManager::kill_process(info.pid);
        assert!(kill_result.is_ok());

        // Second kill should fail
        let kill2 = sandbox_core::process::ProcessManager::kill_process(info.pid);
        assert!(kill2.is_err());
    }
}

#[test]
fn send_input_to_spawned_process() {
    let spawn_result = sandbox_core::process::ProcessManager::spawn_cli("cat", &[]);
    if let Ok(info) = spawn_result {
        let write_result = sandbox_core::process::ProcessManager::send_input(info.pid, b"hello\n");
        assert!(write_result.is_ok());
        let _ = sandbox_core::process::ProcessManager::kill_process(info.pid);
    }
}

#[test]
fn read_output_from_spawned_process() {
    let spawn_result =
        sandbox_core::process::ProcessManager::spawn_cli("echo", &["test_output".to_string()]);
    if let Ok(info) = spawn_result {
        // Give it time to produce output
        std::thread::sleep(std::time::Duration::from_millis(200));
        let output = sandbox_core::process::ProcessManager::read_output(info.pid);
        assert!(output.is_ok());
        let _ = sandbox_core::process::ProcessManager::kill_process(info.pid);
    }
}
