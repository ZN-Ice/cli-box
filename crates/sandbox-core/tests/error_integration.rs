use sandbox_core::AppError;

#[test]
fn error_display_is_descriptive() {
    assert_eq!(
        AppError::WindowNotFound("w42".into()).to_string(),
        "Window not found: w42"
    );
    assert_eq!(
        AppError::Process("oom".into()).to_string(),
        "Process error: oom"
    );
    assert_eq!(
        AppError::Input("bad key".into()).to_string(),
        "Input injection failed: bad key"
    );
    assert_eq!(
        AppError::SandboxNotInitialized.to_string(),
        "Sandbox not initialized"
    );
}

#[test]
fn io_error_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let app_err: AppError = io_err.into();
    assert!(app_err.to_string().contains("file missing"));
}

#[test]
fn json_error_conversion() {
    let json_err: Result<serde_json::Value, serde_json::Error> = serde_json::from_str("{bad json}");
    let err = json_err.unwrap_err();
    let app_err: AppError = err.into();
    assert!(app_err.to_string().contains("JSON error"));
}

#[test]
#[allow(clippy::unnecessary_literal_unwrap)]
fn result_type_alias() {
    let ok: sandbox_core::Result<i32> = Ok(42);
    assert!(ok.is_ok());
    assert_eq!(ok.expect("should be Ok"), 42);

    let err: sandbox_core::Result<i32> = Err(AppError::SandboxNotInitialized);
    assert!(err.is_err());
}

#[test]
fn error_is_send_sync() {
    // AppError should be Send + Sync for use with anyhow/tokio
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AppError>();
}
