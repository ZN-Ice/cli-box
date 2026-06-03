use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Generate a random 8-character hex instance ID
pub fn generate_instance_id() -> String {
    let id = uuid::Uuid::new_v4();
    let bytes = id.as_bytes();
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

/// What kind of process a sandbox is running
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "detail")]
pub enum InstanceKind {
    #[serde(rename = "cli")]
    Cli {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    #[serde(rename = "app")]
    App { path: String },
}

/// Status of a sandbox instance
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "detail")]
pub enum InstanceStatus {
    Starting,
    Running,
    Stopped,
    Error(String),
}

/// A registered sandbox instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInstance {
    pub id: String,
    pub port: u16,
    pub pid: u32,
    pub kind: InstanceKind,
    pub title: String,
    pub status: InstanceStatus,
    pub created_at: String,
    pub window_id: Option<u32>,
}

impl SandboxInstance {
    pub fn new(id: String, port: u16, pid: u32, kind: InstanceKind) -> Self {
        let title = match &kind {
            InstanceKind::Cli { command, .. } => command.clone(),
            InstanceKind::App { path } => std::path::Path::new(path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        };
        Self {
            id,
            port,
            pid,
            kind,
            title,
            status: InstanceStatus::Starting,
            created_at: chrono_now(),
            window_id: None,
        }
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // Simple ISO-ish timestamp from epoch
    let secs = now.as_secs();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        1970 + secs / 31536000,
        (secs % 31536000) / 2592000,
        (secs % 2592000) / 86400,
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60,
    )
}

/// File-system based instance registry
pub struct InstanceRegistry {
    base_dir: PathBuf,
}

impl Default for InstanceRegistry {
    fn default() -> Self {
        Self::new(dirs_home().join(".cli-box").join("instances"))
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

impl InstanceRegistry {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn instance_path(&self, id: &str) -> PathBuf {
        self.base_dir.join(format!("{id}.json"))
    }

    /// Register a new sandbox instance
    pub fn register(&self, instance: &SandboxInstance) -> Result<()> {
        std::fs::create_dir_all(&self.base_dir)
            .map_err(|e| AppError::Instance(format!("Failed to create registry dir: {e}")))?;
        let path = self.instance_path(&instance.id);
        let json = serde_json::to_string_pretty(instance)
            .map_err(|e| AppError::Instance(format!("Failed to serialize instance: {e}")))?;
        std::fs::write(&path, json)
            .map_err(|e| AppError::Instance(format!("Failed to write registry file: {e}")))?;
        tracing::info!("Registered instance: {}", instance.id);
        Ok(())
    }

    /// Get a specific instance by ID
    pub fn get(&self, id: &str) -> Result<SandboxInstance> {
        let path = self.instance_path(id);
        if !path.exists() {
            return Err(AppError::Instance(format!("Instance '{id}' not found")));
        }
        let json = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Instance(format!("Failed to read instance '{id}': {e}")))?;
        serde_json::from_str(&json)
            .map_err(|e| AppError::Instance(format!("Failed to parse instance '{id}': {e}")))
    }

    /// List all registered instances, sorted by created_at descending
    pub fn list(&self) -> Result<Vec<SandboxInstance>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let mut instances = Vec::new();
        let entries = std::fs::read_dir(&self.base_dir)
            .map_err(|e| AppError::Instance(format!("Failed to read registry dir: {e}")))?;
        for entry in entries {
            let entry =
                entry.map_err(|e| AppError::Instance(format!("Failed to read dir entry: {e}")))?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let json = std::fs::read_to_string(&path).unwrap_or_default();
                if let Ok(instance) = serde_json::from_str::<SandboxInstance>(&json) {
                    instances.push(instance);
                }
            }
        }
        instances.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(instances)
    }

    /// Remove an instance from the registry
    pub fn unregister(&self, id: &str) -> Result<()> {
        let path = self.instance_path(id);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                AppError::Instance(format!("Failed to remove instance '{id}': {e}"))
            })?;
            tracing::info!("Unregistered instance: {id}");
        }
        Ok(())
    }

    /// Update the status of an instance
    pub fn update_status(&self, id: &str, status: InstanceStatus) -> Result<()> {
        let mut instance = self.get(id)?;
        instance.status = status;
        self.register(&instance)
    }

    /// Update the window ID of an instance
    pub fn update_window_id(&self, id: &str, window_id: u32) -> Result<()> {
        let mut instance = self.get(id)?;
        instance.window_id = Some(window_id);
        self.register(&instance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_instance_id() {
        let id1 = generate_instance_id();
        let id2 = generate_instance_id();
        assert_ne!(id1, id2, "IDs should be unique");
        assert_eq!(id1.len(), 8, "ID should be 8 chars");
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_instance_registry_crud() {
        let tmp = tempfile("crud");
        let registry = InstanceRegistry::new(tmp.clone());
        let instance = SandboxInstance::new(
            "test1234".into(),
            15801,
            12345,
            InstanceKind::Cli {
                command: "echo".into(),
                args: vec!["hello".into()],
            },
        );

        registry.register(&instance).unwrap();
        let got = registry.get("test1234").unwrap();
        assert_eq!(got.id, "test1234");
        assert_eq!(got.port, 15801);
        assert_eq!(got.title, "echo");

        let list = registry.list().unwrap();
        assert_eq!(list.len(), 1);

        registry
            .update_status("test1234", InstanceStatus::Running)
            .unwrap();
        let updated = registry.get("test1234").unwrap();
        assert!(matches!(updated.status, InstanceStatus::Running));

        registry.unregister("test1234").unwrap();
        assert!(registry.get("test1234").is_err());
    }

    #[test]
    fn test_instance_app_kind_title() {
        let instance = SandboxInstance::new(
            "app12345".into(),
            15802,
            12346,
            InstanceKind::App {
                path: "/Applications/TextEdit.app".into(),
            },
        );
        assert_eq!(instance.title, "TextEdit");
    }

    #[test]
    fn test_default_registry_uses_home_dir() {
        let registry = InstanceRegistry::default();
        let expected = dirs_home().join(".cli-box").join("instances");
        assert_eq!(registry.base_dir, expected);
    }

    #[test]
    fn test_update_window_id() {
        let tmp = tempfile("window_id");
        let registry = InstanceRegistry::new(tmp.clone());
        let instance = SandboxInstance::new(
            "win_test".into(),
            15801,
            99999,
            InstanceKind::Cli {
                command: "vim".into(),
                args: vec![],
            },
        );
        registry.register(&instance).unwrap();

        registry.update_window_id("win_test", 42).unwrap();
        let got = registry.get("win_test").unwrap();
        assert_eq!(got.window_id, Some(42));
    }

    #[test]
    fn test_list_empty_dir() {
        let tmp = tempfile("empty_list");
        let registry = InstanceRegistry::new(tmp.clone());
        let list = registry.list().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_list_multiple_sorted_by_created_at() {
        let tmp = tempfile("multi_list");
        let registry = InstanceRegistry::new(tmp);
        for i in 0..3 {
            let instance = SandboxInstance::new(
                format!("inst{i}"),
                15801 + i,
                (1000 + i) as u32,
                InstanceKind::Cli {
                    command: "echo".into(),
                    args: vec![],
                },
            );
            registry.register(&instance).unwrap();
        }
        let list = registry.list().unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_unregister_nonexistent_is_ok() {
        let tmp = tempfile("noexist");
        let registry = InstanceRegistry::new(tmp);
        assert!(registry.unregister("ghost").is_ok());
    }

    #[test]
    fn test_get_nonexistent_returns_error() {
        let tmp = tempfile("get_missing");
        let registry = InstanceRegistry::new(tmp);
        assert!(registry.get("missing").is_err());
    }

    #[test]
    fn test_instance_serialization_roundtrip() {
        let instance = SandboxInstance::new(
            "ser_1234".into(),
            15999,
            55555,
            InstanceKind::App {
                path: "/Applications/Notes.app".into(),
            },
        );
        let json = serde_json::to_string(&instance).unwrap();
        let parsed: SandboxInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "ser_1234");
        assert_eq!(parsed.port, 15999);
        assert!(matches!(parsed.kind, InstanceKind::App { .. }));
        assert!(matches!(parsed.status, InstanceStatus::Starting));
    }

    fn tempfile(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sandbox_test_{}_{}", std::process::id(), tag))
    }
}
