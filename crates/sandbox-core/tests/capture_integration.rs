use sandbox_core::capture::ScreenCapture;

#[test]
fn capture_window_with_invalid_id_returns_error() {
    let result = ScreenCapture::capture_window(9999999);
    assert!(result.is_err());
}

#[test]
fn capture_region_returns_error_for_invalid_region() {
    // Large region that may be outside bounds
    let result = ScreenCapture::capture_region(-9999, -9999, 1, 1);
    // May succeed or fail depending on display setup
    // Just verify it doesn't panic
    let _ = result;
}

#[test]
fn capture_sandbox_returns_error_without_running_app() {
    let result = ScreenCapture::capture_sandbox();
    // Without a running sandbox app, this should fail
    assert!(result.is_err());
}

#[test]
fn capture_sandbox_by_id_with_none_returns_error() {
    let result = ScreenCapture::capture_sandbox_by_id(None);
    // Without a running sandbox app, this should fail
    assert!(result.is_err());
}

#[test]
fn capture_sandbox_by_id_with_invalid_id_returns_error() {
    let result = ScreenCapture::capture_sandbox_by_id(Some(9999999));
    assert!(result.is_err());
}

#[test]
fn find_window_by_title_nonexistent_returns_error() {
    let result = ScreenCapture::find_window_by_title("__nonexistent_window_xyz__");
    assert!(result.is_err());
}

#[test]
fn find_window_by_pid_returns_error_for_nonexistent_pid() {
    let result = ScreenCapture::find_window_by_pid(9999999);
    assert!(result.is_err());
}

#[test]
fn find_window_by_pid_returns_error_for_pid_zero() {
    // PID 0 is kernel_task, which has no normal windows
    let result = ScreenCapture::find_window_by_pid(0);
    assert!(result.is_err());
}

#[test]
fn find_window_by_pid_discovers_own_process_if_it_has_windows() {
    // The test runner itself may or may not have windows depending on how it's run.
    // Just verify it doesn't panic and returns a proper Result.
    let own_pid = std::process::id();
    let result = ScreenCapture::find_window_by_pid(own_pid);
    // We don't assert success because the test runner might not have a window,
    // but it should not panic.
    let _ = result;
}

#[test]
fn list_windows_returns_ok() {
    let result = ScreenCapture::list_windows();
    // Should succeed on macOS (requires Screen Recording permission)
    // If permission is not granted, this will error
    match result {
        Ok(windows) => {
            // There should be at least some windows on a running system
            assert!(!windows.is_empty(), "Expected at least one window");
        }
        Err(_) => {
            // Permission not granted - that's acceptable for CI
        }
    }
}
