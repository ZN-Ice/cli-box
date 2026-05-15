use crate::error::{AppError, Result};

/// Screenshot engine using ScreenCaptureKit (macOS)
pub struct ScreenCapture;

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::*;
    use screencapturekit::screenshot_manager::SCScreenshotManager;
    use screencapturekit::shareable_content::SCShareableContent;
    use screencapturekit::stream::configuration::SCStreamConfiguration;
    use screencapturekit::stream::content_filter::SCContentFilter;

    impl ScreenCapture {
        /// Capture a specific window by its SCWindow ID.
        /// Returns PNG-encoded image bytes.
        /// Works even when the window is behind other windows.
        pub fn capture_window(window_id: u32) -> Result<Vec<u8>> {
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {:?}", e))
            })?;

            let windows = content.windows();
            let window = windows
                .iter()
                .find(|w| w.window_id() == window_id)
                .ok_or_else(|| {
                    AppError::WindowNotFound(format!("SCWindow ID {} not found", window_id))
                })?;

            let filter = SCContentFilter::create().with_window(window).build();

            let config = SCStreamConfiguration::new()
                .with_width(window.frame().width as u32)
                .with_height(window.frame().height as u32);

            let image = SCScreenshotManager::capture_image(&filter, &config)
                .map_err(|e| AppError::Screenshot(format!("Failed to capture image: {:?}", e)))?;

            let rgba = image
                .rgba_data()
                .map_err(|e| AppError::Screenshot(format!("Failed to get RGBA data: {:?}", e)))?;

            rgba_to_png(&rgba, image.width(), image.height())
        }

        /// Capture a region of a display
        pub fn capture_region(_x: i32, _y: i32, width: u32, height: u32) -> Result<Vec<u8>> {
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {:?}", e))
            })?;

            let displays = content.displays();
            let display = displays
                .first()
                .ok_or_else(|| AppError::Screenshot("No display found".into()))?;

            let filter = SCContentFilter::create()
                .with_display(display)
                .with_excluding_windows(&[])
                .build();

            let config = SCStreamConfiguration::new()
                .with_width(width)
                .with_height(height);

            let image = SCScreenshotManager::capture_image(&filter, &config)
                .map_err(|e| AppError::Screenshot(format!("Failed to capture region: {:?}", e)))?;

            let rgba = image
                .rgba_data()
                .map_err(|e| AppError::Screenshot(format!("Failed to get RGBA data: {:?}", e)))?;

            rgba_to_png(&rgba, image.width(), image.height())
        }

        /// Capture the sandbox window by searching for it by title
        pub fn capture_sandbox() -> Result<Vec<u8>> {
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {:?}", e))
            })?;

            let window_list = content.windows();
            let window = window_list
                .iter()
                .find(|w| {
                    w.title()
                        .map(|t| t.contains("System Test Sandbox"))
                        .unwrap_or(false)
                })
                .ok_or_else(|| AppError::WindowNotFound("Sandbox window not found".into()))?;

            let filter = SCContentFilter::create().with_window(window).build();

            let config = SCStreamConfiguration::new()
                .with_width(1280)
                .with_height(800);

            let image = SCScreenshotManager::capture_image(&filter, &config)
                .map_err(|e| AppError::Screenshot(format!("Failed to capture sandbox: {:?}", e)))?;

            let rgba = image
                .rgba_data()
                .map_err(|e| AppError::Screenshot(format!("Failed to get RGBA data: {:?}", e)))?;

            rgba_to_png(&rgba, image.width(), image.height())
        }

        /// Find a window by title substring
        pub fn find_window_by_title(title: &str) -> Result<u32> {
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {:?}", e))
            })?;

            let window_list = content.windows();
            window_list
                .iter()
                .find(|w| w.title().map(|t| t.contains(title)).unwrap_or(false))
                .map(|w| w.window_id())
                .ok_or_else(|| AppError::WindowNotFound(format!("Window '{}' not found", title)))
        }

        /// List all available windows with their IDs and titles
        pub fn list_windows() -> Result<Vec<(u32, String)>> {
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {:?}", e))
            })?;

            let window_list = content.windows();
            let windows: Vec<(u32, String)> = window_list
                .iter()
                .map(|w| (w.window_id(), w.title().unwrap_or_default().to_string()))
                .collect();
            Ok(windows)
        }
    }

    /// Convert RGBA pixel data to PNG bytes using the image crate
    fn rgba_to_png(rgba: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
        use image::{ImageBuffer, RgbaImage};
        use std::io::Cursor;

        let img: RgbaImage = ImageBuffer::from_raw(width as u32, height as u32, rgba.to_vec())
            .ok_or_else(|| {
                AppError::Screenshot("Failed to create image buffer from RGBA data".into())
            })?;

        let mut cursor = Cursor::new(Vec::new());
        img.write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| AppError::Screenshot(format!("Failed to encode PNG: {}", e)))?;

        Ok(cursor.into_inner())
    }
}

#[cfg(not(target_os = "macos"))]
mod non_macos_impl {
    use super::*;

    impl ScreenCapture {
        pub fn capture_window(_window_id: u32) -> Result<Vec<u8>> {
            Err(AppError::Screenshot(
                "ScreenCaptureKit only available on macOS".into(),
            ))
        }

        pub fn capture_region(_x: i32, _y: i32, _width: u32, _height: u32) -> Result<Vec<u8>> {
            Err(AppError::Screenshot(
                "ScreenCaptureKit only available on macOS".into(),
            ))
        }

        pub fn capture_sandbox() -> Result<Vec<u8>> {
            Err(AppError::Screenshot(
                "ScreenCaptureKit only available on macOS".into(),
            ))
        }

        pub fn find_window_by_title(_title: &str) -> Result<u32> {
            Err(AppError::Screenshot(
                "ScreenCaptureKit only available on macOS".into(),
            ))
        }

        pub fn list_windows() -> Result<Vec<(u32, String)>> {
            Err(AppError::Screenshot(
                "ScreenCaptureKit only available on macOS".into(),
            ))
        }
    }
}
