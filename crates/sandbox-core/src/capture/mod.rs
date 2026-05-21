use crate::error::{AppError, Result};

/// Screenshot engine using ScreenCaptureKit (macOS)
pub struct ScreenCapture;

#[cfg(all(target_os = "macos", feature = "screencapturekit"))]
mod macos_impl {
    use super::*;
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::number::CFNumber;
    use core_foundation::number::CFNumberRef;
    use core_foundation::string::CFString;
    use screencapturekit::screenshot_manager::SCScreenshotManager;
    use screencapturekit::shareable_content::SCShareableContent;
    use screencapturekit::stream::configuration::SCStreamConfiguration;
    use screencapturekit::stream::content_filter::SCContentFilter;
    use std::os::raw::c_void;
    use std::sync::Once;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDictionaryGetValueIfPresent(
            dict: CFDictionaryRef,
            key: *const c_void,
            value: *mut *const c_void,
        ) -> bool;
    }

    static CG_INIT: Once = Once::new();

    /// Ensure CoreGraphics is initialized before ScreenCaptureKit calls.
    /// Without this, `SCShareableContent::get()` (async path) triggers
    /// `CGS_REQUIRE_INIT` assertion when run from non-GUI context.
    fn ensure_cg_initialized() {
        CG_INIT.call_once(|| unsafe {
            screencapturekit::ffi::sc_initialize_core_graphics();
        });
    }

    impl ScreenCapture {
        /// Capture a specific window by its SCWindow ID.
        /// Returns PNG-encoded image bytes.
        /// Works even when the window is behind other windows.
        pub fn capture_window(window_id: u32) -> Result<Vec<u8>> {
            ensure_cg_initialized();
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {e:?}"))
            })?;

            let windows = content.windows();
            let window = windows
                .iter()
                .find(|w| w.window_id() == window_id)
                .ok_or_else(|| {
                    AppError::WindowNotFound(format!("SCWindow ID {window_id} not found"))
                })?;

            let filter = SCContentFilter::create().with_window(window).build();

            let config = SCStreamConfiguration::new()
                .with_width(window.frame().width as u32)
                .with_height(window.frame().height as u32);

            let image = SCScreenshotManager::capture_image(&filter, &config)
                .map_err(|e| AppError::Screenshot(format!("Failed to capture image: {e:?}")))?;

            let rgba = image
                .rgba_data()
                .map_err(|e| AppError::Screenshot(format!("Failed to get RGBA data: {e:?}")))?;

            rgba_to_png(&rgba, image.width(), image.height())
        }

        /// Capture a region of a display at the given screen coordinates.
        /// Captures the full display and crops to (x, y, width, height) using the image crate.
        pub fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<Vec<u8>> {
            ensure_cg_initialized();
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {e:?}"))
            })?;

            let displays = content.displays();
            let display = displays
                .first()
                .ok_or_else(|| AppError::Screenshot("No display found".into()))?;

            let frame = display.frame();
            let display_w = frame.width as u32;
            let display_h = frame.height as u32;

            let filter = SCContentFilter::create()
                .with_display(display)
                .with_excluding_windows(&[])
                .build();

            let config = SCStreamConfiguration::new()
                .with_width(display_w)
                .with_height(display_h);

            let image = SCScreenshotManager::capture_image(&filter, &config)
                .map_err(|e| AppError::Screenshot(format!("Failed to capture region: {e:?}")))?;

            let rgba = image
                .rgba_data()
                .map_err(|e| AppError::Screenshot(format!("Failed to get RGBA data: {e:?}")))?;

            // Crop to the requested region using the image crate
            crop_rgba(&rgba, image.width(), image.height(), x, y, width, height)
        }

        /// Capture the sandbox window by searching for it by title
        pub fn capture_sandbox() -> Result<Vec<u8>> {
            ensure_cg_initialized();
            Self::capture_sandbox_by_id(None)
        }

        /// Capture the sandbox window, optionally by a specific window ID.
        /// If window_id is None, searches for a window titled "System Test Sandbox".
        pub fn capture_sandbox_by_id(window_id: Option<u32>) -> Result<Vec<u8>> {
            ensure_cg_initialized();
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {e:?}"))
            })?;

            let window_list = content.windows();

            let window = if let Some(id) = window_id {
                // Use the provided window ID directly
                window_list
                    .iter()
                    .find(|w| w.window_id() == id)
                    .ok_or_else(|| AppError::WindowNotFound(format!("Window ID {id} not found")))?
            } else {
                // Fallback: search by title
                window_list
                    .iter()
                    .find(|w| {
                        w.title()
                            .map(|t| t.contains("System Test Sandbox"))
                            .unwrap_or(false)
                    })
                    .ok_or_else(|| {
                        AppError::WindowNotFound(
                            "Sandbox window not found. In CLI mode, use capture_window(window_id) \
                             or start the Tauri app first."
                                .into(),
                        )
                    })?
            };

            let filter = SCContentFilter::create().with_window(window).build();

            let config = SCStreamConfiguration::new()
                .with_width(window.frame().width as u32)
                .with_height(window.frame().height as u32);

            let image = SCScreenshotManager::capture_image(&filter, &config)
                .map_err(|e| AppError::Screenshot(format!("Failed to capture sandbox: {e:?}")))?;

            let rgba = image
                .rgba_data()
                .map_err(|e| AppError::Screenshot(format!("Failed to get RGBA data: {e:?}")))?;

            rgba_to_png(&rgba, image.width(), image.height())
        }

        /// Find a window by title substring
        pub fn find_window_by_title(title: &str) -> Result<u32> {
            ensure_cg_initialized();
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {e:?}"))
            })?;

            let window_list = content.windows();
            window_list
                .iter()
                .find(|w| w.title().map(|t| t.contains(title)).unwrap_or(false))
                .map(|w| w.window_id())
                .ok_or_else(|| AppError::WindowNotFound(format!("Window '{title}' not found")))
        }

        /// Find the main window owned by a specific PID using CGWindowListCopyWindowInfo.
        /// Returns the CGWindowNumber, which matches SCWindow::window_id().
        /// Picks the largest on-screen window with layer 0 (normal window layer).
        pub fn find_window_by_pid(pid: u32) -> Result<u32> {
            ensure_cg_initialized();
            unsafe {
                let arr_ref = CGWindowListCopyWindowInfo(0, 0);
                if arr_ref.is_null() {
                    return Err(AppError::WindowNotFound(
                        "CGWindowListCopyWindowInfo returned null".into(),
                    ));
                }
                let arr = CFArray::<*const c_void>::wrap_under_create_rule(arr_ref);

                let mut best_id: Option<u32> = None;
                let mut best_area: u64 = 0;

                for i in 0..arr.len() {
                    let item_ref = match arr.get(i) {
                        Some(p) => p,
                        None => continue,
                    };
                    let item_ptr: *const c_void = *item_ref;
                    if item_ptr.is_null() {
                        continue;
                    }
                    let dict = item_ptr as CFDictionaryRef;

                    // Check kCGWindowOwnerPID
                    let key_pid = CFString::new("kCGWindowOwnerPID");
                    let mut val_ptr: *const c_void = std::ptr::null();
                    if !CFDictionaryGetValueIfPresent(
                        dict,
                        key_pid.as_concrete_TypeRef() as *const c_void,
                        &mut val_ptr as *mut _,
                    ) || val_ptr.is_null()
                    {
                        continue;
                    }
                    let owner_pid = CFNumber::wrap_under_get_rule(val_ptr as CFNumberRef)
                        .to_i64()
                        .unwrap_or(-1) as u32;
                    if owner_pid != pid {
                        continue;
                    }

                    // Skip non-normal layers (layer 0 = normal window)
                    let key_layer = CFString::new("kCGWindowLayer");
                    let mut layer_ptr: *const c_void = std::ptr::null();
                    if CFDictionaryGetValueIfPresent(
                        dict,
                        key_layer.as_concrete_TypeRef() as *const c_void,
                        &mut layer_ptr as *mut _,
                    ) && !layer_ptr.is_null()
                    {
                        let layer = CFNumber::wrap_under_get_rule(layer_ptr as CFNumberRef)
                            .to_i64()
                            .unwrap_or(0);
                        if layer != 0 {
                            continue;
                        }
                    }

                    // Get kCGWindowNumber
                    let key_num = CFString::new("kCGWindowNumber");
                    let mut num_ptr: *const c_void = std::ptr::null();
                    if !CFDictionaryGetValueIfPresent(
                        dict,
                        key_num.as_concrete_TypeRef() as *const c_void,
                        &mut num_ptr as *mut _,
                    ) || num_ptr.is_null()
                    {
                        continue;
                    }
                    let win_id = CFNumber::wrap_under_get_rule(num_ptr as CFNumberRef)
                        .to_i64()
                        .unwrap_or(0) as u32;

                    // Compute area from kCGWindowBounds to pick the largest window
                    let key_bounds = CFString::new("kCGWindowBounds");
                    let mut bounds_ptr: *const c_void = std::ptr::null();
                    if CFDictionaryGetValueIfPresent(
                        dict,
                        key_bounds.as_concrete_TypeRef() as *const c_void,
                        &mut bounds_ptr as *mut _,
                    ) && !bounds_ptr.is_null()
                    {
                        let bdict = bounds_ptr as CFDictionaryRef;
                        let key_w = CFString::new("Width");
                        let key_h = CFString::new("Height");
                        let mut w_ptr: *const c_void = std::ptr::null();
                        let mut h_ptr: *const c_void = std::ptr::null();
                        if CFDictionaryGetValueIfPresent(
                            bdict,
                            key_w.as_concrete_TypeRef() as *const c_void,
                            &mut w_ptr as *mut _,
                        ) && CFDictionaryGetValueIfPresent(
                            bdict,
                            key_h.as_concrete_TypeRef() as *const c_void,
                            &mut h_ptr as *mut _,
                        ) && !w_ptr.is_null()
                            && !h_ptr.is_null()
                        {
                            let w = CFNumber::wrap_under_get_rule(w_ptr as CFNumberRef)
                                .to_i64()
                                .unwrap_or(0) as u64;
                            let h = CFNumber::wrap_under_get_rule(h_ptr as CFNumberRef)
                                .to_i64()
                                .unwrap_or(0) as u64;
                            if w * h > best_area {
                                best_area = w * h;
                                best_id = Some(win_id);
                            }
                            continue;
                        }
                    }

                    // No bounds — use as fallback if nothing better found
                    if best_id.is_none() {
                        best_id = Some(win_id);
                    }
                }

                best_id.ok_or_else(|| {
                    AppError::WindowNotFound(format!("No window found for PID {pid}"))
                })
            }
        }

        /// List all available windows with their IDs and titles
        pub fn list_windows() -> Result<Vec<(u32, String)>> {
            ensure_cg_initialized();
            let content = SCShareableContent::get().map_err(|e| {
                AppError::Screenshot(format!("Failed to get shareable content: {e:?}"))
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
            .map_err(|e| AppError::Screenshot(format!("Failed to encode PNG: {e}")))?;

        Ok(cursor.into_inner())
    }

    /// Crop RGBA pixel data to the specified region, then encode as PNG.
    fn crop_rgba(
        rgba: &[u8],
        full_width: usize,
        full_height: usize,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        use image::imageops;
        use image::{ImageBuffer, RgbaImage};
        use std::io::Cursor;

        let mut img: RgbaImage =
            ImageBuffer::from_raw(full_width as u32, full_height as u32, rgba.to_vec())
                .ok_or_else(|| {
                    AppError::Screenshot("Failed to create image buffer from RGBA data".into())
                })?;

        let crop_x = x.max(0) as u32;
        let crop_y = y.max(0) as u32;
        let crop_w = width.min(full_width as u32 - crop_x);
        let crop_h = height.min(full_height as u32 - crop_y);

        if crop_w == 0 || crop_h == 0 {
            return Err(AppError::Screenshot(format!(
                "Crop region ({x}, {y}, {width}x{height}) is outside display bounds ({full_width}x{full_height})"
            )));
        }

        let cropped = imageops::crop(&mut img, crop_x, crop_y, crop_w, crop_h);
        let cropped_img = cropped.to_image();

        let mut cursor = Cursor::new(Vec::new());
        cropped_img
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| AppError::Screenshot(format!("Failed to encode cropped PNG: {e}")))?;

        Ok(cursor.into_inner())
    }
}

#[cfg(any(not(target_os = "macos"), not(feature = "screencapturekit")))]
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

        pub fn capture_sandbox_by_id(_window_id: Option<u32>) -> Result<Vec<u8>> {
            Err(AppError::Screenshot(
                "ScreenCaptureKit only available on macOS".into(),
            ))
        }

        pub fn find_window_by_title(_title: &str) -> Result<u32> {
            Err(AppError::Screenshot(
                "ScreenCaptureKit only available on macOS".into(),
            ))
        }

        pub fn find_window_by_pid(_pid: u32) -> Result<u32> {
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
