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
