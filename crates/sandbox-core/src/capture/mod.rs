use crate::error::Result;

/// Screenshot engine using ScreenCaptureKit (macOS)
pub struct ScreenCapture;

impl ScreenCapture {
    /// Capture a specific window by its window ID.
    /// Returns base64-encoded PNG data.
    /// Works even when the window is behind other windows.
    pub fn capture_window(window_id: u32) -> Result<Vec<u8>> {
        let _ = window_id;
        todo!("ScreenCaptureKit window capture")
    }

    /// Capture a region of the screen
    pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<Vec<u8>> {
        let _ = (x, y, width, height);
        todo!("ScreenCaptureKit region capture")
    }

    /// Capture the entire sandbox window
    pub fn capture_sandbox() -> Result<Vec<u8>> {
        todo!("ScreenCaptureKit sandbox capture")
    }
}
