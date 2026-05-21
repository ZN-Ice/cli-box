use sandbox_core::instance::{
    generate_instance_id, InstanceKind, InstanceRegistry, InstanceStatus, SandboxInstance,
};
use std::path::PathBuf;

#[test]
fn instance_kind_cli_serialization() {
    let kind = InstanceKind::Cli {
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
    };
    let json = serde_json::to_string(&kind).unwrap();
    assert!(json.contains("\"type\":\"cli\""));
    assert!(json.contains("\"detail\""));

    let parsed: InstanceKind = serde_json::from_str(&json).unwrap();
    assert!(matches!(parsed, InstanceKind::Cli { .. }));
}

#[test]
fn instance_kind_app_serialization() {
    let kind = InstanceKind::App {
        path: "/Applications/Safari.app".to_string(),
    };
    let json = serde_json::to_string(&kind).unwrap();
    assert!(json.contains("\"type\":\"app\""));

    let parsed: InstanceKind = serde_json::from_str(&json).unwrap();
    assert!(matches!(parsed, InstanceKind::App { .. }));
}

#[test]
fn instance_status_variants() {
    let variants = vec![
        InstanceStatus::Starting,
        InstanceStatus::Running,
        InstanceStatus::Stopped,
        InstanceStatus::Error("test error".to_string()),
    ];

    for status in variants {
        let json = serde_json::to_string(&status).unwrap();
        let parsed: InstanceStatus = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, json2);
    }
}

#[test]
fn instance_status_error_contains_message() {
    let status = InstanceStatus::Error("something went wrong".to_string());
    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("something went wrong"));
}

#[test]
fn instance_created_at_is_set() {
    let instance = SandboxInstance::new(
        "test1234".into(),
        5801,
        1234,
        InstanceKind::Cli {
            command: "echo".into(),
            args: vec![],
        },
    );
    assert!(!instance.created_at.is_empty());
    // Should be a timestamp-like format
    assert!(instance.created_at.contains("-"));
}

#[test]
fn registry_with_corrupted_json_file() {
    let tmp = PathBuf::from(format!("/tmp/sandbox_test_corrupt_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    // Write a corrupted JSON file
    std::fs::write(tmp.join("bad.json"), "not valid json").unwrap();

    let registry = InstanceRegistry::new(tmp.clone());
    let list = registry.list().unwrap();
    assert!(list.is_empty()); // corrupted file is skipped

    // Clean up
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn registry_with_non_json_file() {
    let tmp = PathBuf::from(format!("/tmp/sandbox_test_nonjson_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    // Write a non-JSON file
    std::fs::write(tmp.join("notes.txt"), "some text").unwrap();

    let registry = InstanceRegistry::new(tmp.clone());
    let list = registry.list().unwrap();
    assert!(list.is_empty()); // non-.json files are ignored

    // Clean up
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn instance_window_id_default_is_none() {
    let instance = SandboxInstance::new(
        "test_id".into(),
        5801,
        999,
        InstanceKind::Cli {
            command: "bash".into(),
            args: vec![],
        },
    );
    assert!(instance.window_id.is_none());
}

#[test]
fn multiple_generate_instance_ids_are_unique() {
    let ids: std::collections::HashSet<String> = (0..100).map(|_| generate_instance_id()).collect();
    assert_eq!(ids.len(), 100, "All generated IDs should be unique");
}

#[test]
fn instance_kind_cli_with_empty_args() {
    let kind = InstanceKind::Cli {
        command: "ls".to_string(),
        args: vec![],
    };
    let json = serde_json::to_string(&kind).unwrap();
    let parsed: InstanceKind = serde_json::from_str(&json).unwrap();
    if let InstanceKind::Cli { args, .. } = parsed {
        assert!(args.is_empty());
    } else {
        panic!("Expected Cli variant");
    }
}
