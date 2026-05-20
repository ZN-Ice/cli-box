use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// UI element representation from Accessibility API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    pub role: String,
    pub title: Option<String>,
    pub value: Option<String>,
    pub description: Option<String>,
    pub bounds: Option<Bounds>,
    pub children: Vec<UiElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// UI inspector using AXUIElement (macOS Accessibility API)
pub struct UiInspector;

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::*;
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFGetTypeID, CFRelease, CFRetain, CFTypeRef, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::number::CFNumber;
    use core_foundation::string::{CFString, CFStringRef};
    use std::os::raw::c_void;

    // Type aliases from core-foundation (re-exported from core-foundation-sys)
    use core_foundation::array::CFArrayRef;
    use core_foundation::boolean::CFBooleanRef;
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::number::CFNumberRef;

    type AXUIElementRef = *const c_void;
    type AXError = i32;

    const K_AX_ERROR_SUCCESS: AXError = 0;

    /// Minimum pointer address to be considered a valid AXUIElementRef.
    /// Anything below this is almost certainly a null offset or dangling pointer.
    const MIN_VALID_PTR: usize = 0x1000;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
        ) -> CFTypeRef;
        fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> AXError;
        fn AXIsProcessTrusted() -> bool;
    }

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

    fn ax_attr(s: &str) -> CFString {
        CFString::new(s)
    }

    /// Safe wrapper around AXUIElementCopyAttributeValue.
    /// Validates the element pointer before calling to prevent SIGSEGV.
    /// Returns None if the element is invalid or the call fails.
    unsafe fn ax_try_copy_value(element: AXUIElementRef, attr_name: &str) -> Option<CFTypeRef> {
        if element.is_null() {
            eprintln!("[ax_ui] ax_try_copy_value: null element for attr '{attr_name}'");
            return None;
        }
        if (element as usize) < MIN_VALID_PTR {
            eprintln!(
                "[ax_ui] ax_try_copy_value: suspicious pointer {:#x} for attr '{attr_name}'",
                element as usize
            );
            return None;
        }
        // Secondary validation via AXUIElementGetPid
        let mut pid: i32 = 0;
        let pid_result = AXUIElementGetPid(element, &mut pid as *mut i32);
        if pid_result != K_AX_ERROR_SUCCESS {
            eprintln!(
                "[ax_ui] ax_try_copy_value: AXUIElementGetPid failed (err={}) for attr '{attr_name}'",
                pid_result
            );
            return None;
        }
        let attr = ax_attr(attr_name);
        let raw = AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef());
        if raw.is_null() {
            eprintln!("[ax_ui] ax_try_copy_value: AXUIElementCopyAttributeValue returned null for attr '{attr_name}'");
            return None;
        }
        Some(raw)
    }

    /// Check Accessibility permission and return an error if not granted.
    fn check_accessibility_permission() -> Result<()> {
        if unsafe { AXIsProcessTrusted() } {
            Ok(())
        } else {
            Err(AppError::Accessibility(
                "Accessibility permission not granted. Grant it in System Settings → Privacy & Security → Accessibility.".to_string(),
            ))
        }
    }

    /// CFTypeRef → String conversion
    unsafe fn cf_to_string(raw: CFTypeRef) -> Option<String> {
        if raw.is_null() {
            return None;
        }
        let type_id = CFGetTypeID(raw);
        if type_id == CFString::type_id() {
            let s = CFString::wrap_under_get_rule(raw as CFStringRef);
            Some(s.to_string())
        } else if type_id == CFNumber::type_id() {
            let n = CFNumber::wrap_under_get_rule(raw as CFNumberRef);
            Some(format!("{}", n.to_i64().unwrap_or(0)))
        } else if type_id == CFBoolean::type_id() {
            let b = CFBoolean::wrap_under_get_rule(raw as CFBooleanRef);
            Some(format!("{}", bool::from(b)))
        } else {
            CFRelease(raw);
            None
        }
    }

    unsafe fn ax_get_string(element: AXUIElementRef, attr_name: &str) -> Option<String> {
        let raw = ax_try_copy_value(element, attr_name)?;
        cf_to_string(raw)
    }

    unsafe fn ax_get_children(element: AXUIElementRef) -> Vec<AXUIElementRef> {
        let raw = match ax_try_copy_value(element, "AXChildren") {
            Some(r) => r,
            None => return vec![],
        };
        let arr = CFArray::<*const c_void>::wrap_under_get_rule(raw as CFArrayRef);
        let mut children = Vec::new();
        for i in 0..arr.len() {
            if let Some(ptr_val) = arr.get(i) {
                let val = *ptr_val;
                if !val.is_null() && (val as usize) >= MIN_VALID_PTR {
                    CFRetain(val as CFTypeRef);
                    children.push(val);
                }
            }
        }
        children
    }

    unsafe fn ax_get_attr_array(element: AXUIElementRef, attr_name: &str) -> Vec<AXUIElementRef> {
        let raw = match ax_try_copy_value(element, attr_name) {
            Some(r) => r,
            None => return vec![],
        };
        let arr = CFArray::<*const c_void>::wrap_under_get_rule(raw as CFArrayRef);
        let mut items = Vec::new();
        for i in 0..arr.len() {
            if let Some(ptr_val) = arr.get(i) {
                let val = *ptr_val;
                if !val.is_null() && (val as usize) >= MIN_VALID_PTR {
                    CFRetain(val as CFTypeRef);
                    items.push(val);
                }
            }
        }
        items
    }

    unsafe fn ax_release_all(items: &[AXUIElementRef]) {
        for &item in items {
            if !item.is_null() {
                CFRelease(item as CFTypeRef);
            }
        }
    }

    unsafe fn ax_release_one(element: AXUIElementRef) {
        if !element.is_null() {
            CFRelease(element as CFTypeRef);
        }
    }

    const MAX_UI_DEPTH: usize = 30;

    /// Convert an AXUIElementRef to a UiElement.
    /// Returns None if the element is invalid or an error occurs.
    unsafe fn ax_to_ui_element_safe(element: AXUIElementRef, depth: usize) -> Option<UiElement> {
        if element.is_null() || (element as usize) < MIN_VALID_PTR {
            eprintln!(
                "[ax_ui] ax_to_ui_element_safe: invalid element {:#x} at depth {depth}",
                element as usize
            );
            return None;
        }

        let role = ax_get_string(element, "AXRole").unwrap_or_else(|| "unknown".to_string());
        let title = ax_get_string(element, "AXTitle");
        let value = ax_get_string(element, "AXValue");
        let description = ax_get_string(element, "AXDescription");

        let children = if depth >= MAX_UI_DEPTH {
            vec![]
        } else {
            let children_elements = ax_get_children(element);
            let mut result = Vec::new();
            for &child in &children_elements {
                if let Some(ui_child) = ax_to_ui_element_safe(child, depth + 1) {
                    result.push(ui_child);
                }
            }
            ax_release_all(&children_elements);
            result
        };

        Some(UiElement {
            role,
            title,
            value,
            description,
            bounds: None,
            children,
        })
    }

    unsafe fn ax_find_in_tree(
        element: AXUIElementRef,
        role: Option<&str>,
        title: Option<&str>,
    ) -> Vec<UiElement> {
        match ax_to_ui_element_safe(element, 0) {
            Some(ui) => find_ui_matches(&ui, role, title),
            None => {
                eprintln!("[ax_ui] ax_find_in_tree: failed to convert root element");
                vec![]
            }
        }
    }

    fn find_ui_matches(
        element: &UiElement,
        role: Option<&str>,
        title: Option<&str>,
    ) -> Vec<UiElement> {
        let mut results = Vec::new();
        let matches_role = role.is_none_or(|r| element.role == r);
        let matches_title = title.is_none_or(|t| {
            element
                .title
                .as_deref()
                .is_some_and(|tt| tt.to_lowercase().contains(&t.to_lowercase()))
        });
        if matches_role && matches_title {
            results.push(element.clone());
        }
        for child in &element.children {
            results.extend(find_ui_matches(child, role, title));
        }
        results
    }

    fn get_pid_for_window(window_id: u32) -> Option<i32> {
        unsafe {
            let arr_ref = CGWindowListCopyWindowInfo(0, 0);
            if arr_ref.is_null() {
                return None;
            }
            let arr = CFArray::<*const c_void>::wrap_under_create_rule(arr_ref);

            for i in 0..arr.len() {
                let item_ref = match arr.get(i) {
                    Some(p) => p,
                    None => continue,
                };
                // Deref ItemRef to get the raw pointer, then cast to dictionary ref
                let item_ptr: *const c_void = *item_ref;
                if item_ptr.is_null() {
                    continue;
                }
                let dict = item_ptr as CFDictionaryRef;

                // Look up kCGWindowNumber
                let key_num = CFString::new("kCGWindowNumber");
                let mut val_ptr: *const c_void = std::ptr::null();
                let found = CFDictionaryGetValueIfPresent(
                    dict,
                    key_num.as_concrete_TypeRef() as *const c_void,
                    &mut val_ptr as *mut _,
                );
                if !found || val_ptr.is_null() {
                    continue;
                }
                let num = CFNumber::wrap_under_get_rule(val_ptr as CFNumberRef);
                let win_val = num.to_i64().unwrap_or(0) as u32;
                if win_val != window_id {
                    continue;
                }

                // Look up kCGWindowOwnerPID
                let key_pid = CFString::new("kCGWindowOwnerPID");
                let mut pid_ptr: *const c_void = std::ptr::null();
                let found2 = CFDictionaryGetValueIfPresent(
                    dict,
                    key_pid.as_concrete_TypeRef() as *const c_void,
                    &mut pid_ptr as *mut _,
                );
                if found2 && !pid_ptr.is_null() {
                    let pid_num = CFNumber::wrap_under_get_rule(pid_ptr as CFNumberRef);
                    return Some(pid_num.to_i64().unwrap_or(0) as i32);
                }
            }
            None
        }
    }

    impl UiInspector {
        pub fn inspect_window(window_id: u32) -> Result<UiElement> {
            check_accessibility_permission()?;

            let pid = get_pid_for_window(window_id)
                .ok_or_else(|| AppError::WindowNotFound(format!("Window {window_id} not found")))?;

            eprintln!("[ax_ui] inspect_window: window_id={window_id}, pid={pid}");

            unsafe {
                let app = AXUIElementCreateApplication(pid);
                if app.is_null() {
                    eprintln!("[ax_ui] inspect_window: AXUIElementCreateApplication returned null for pid {pid}");
                    return Err(AppError::Accessibility(
                        "Failed to create AXUIElement for application".to_string(),
                    ));
                }

                let windows = ax_get_attr_array(app, "AXWindows");
                if windows.is_empty() {
                    eprintln!("[ax_ui] inspect_window: no AXWindows for pid {pid}");
                    ax_release_one(app);
                    return Err(AppError::WindowNotFound(format!(
                        "No AXWindows for PID {pid}"
                    )));
                }

                let first_window = windows[0];
                eprintln!(
                    "[ax_ui] inspect_window: first window ptr = {:#x}",
                    first_window as usize
                );

                match ax_to_ui_element_safe(first_window, 0) {
                    Some(ui) => {
                        ax_release_all(&windows);
                        ax_release_one(app);
                        Ok(ui)
                    }
                    None => {
                        eprintln!("[ax_ui] inspect_window: failed to convert window element");
                        ax_release_all(&windows);
                        ax_release_one(app);
                        Err(AppError::Accessibility(
                            "Failed to read UI tree from window".to_string(),
                        ))
                    }
                }
            }
        }

        pub fn find_elements(
            window_id: u32,
            role: Option<&str>,
            title: Option<&str>,
        ) -> Result<Vec<UiElement>> {
            check_accessibility_permission()?;

            let pid = get_pid_for_window(window_id)
                .ok_or_else(|| AppError::WindowNotFound(format!("Window {window_id} not found")))?;

            unsafe {
                let app = AXUIElementCreateApplication(pid);
                if app.is_null() {
                    return Err(AppError::Accessibility(
                        "Failed to create AXUIElement for application".to_string(),
                    ));
                }
                let results = ax_find_in_tree(app, role, title);
                ax_release_one(app);
                Ok(results)
            }
        }

        pub fn get_element_value(element_id: &str) -> Result<Option<String>> {
            check_accessibility_permission()?;

            let parts: Vec<&str> = element_id.split(':').collect();
            if parts.len() < 2 {
                return Err(AppError::Accessibility("Invalid element ID".to_string()));
            }
            let pid: i32 = parts[0]
                .parse()
                .map_err(|_| AppError::Accessibility("Invalid PID in element ID".to_string()))?;

            unsafe {
                let app = AXUIElementCreateApplication(pid);
                if app.is_null() {
                    return Err(AppError::Accessibility(
                        "Failed to create AXUIElement for application".to_string(),
                    ));
                }

                let mut current = app;
                let mut owned: Vec<AXUIElementRef> = vec![app];

                for (idx, part) in parts.iter().enumerate().skip(1) {
                    if part.is_empty() {
                        continue;
                    }
                    let child_idx: usize = part.parse().map_err(|_| {
                        AppError::Accessibility("Invalid element index".to_string())
                    })?;

                    let children = if idx == 1 {
                        ax_get_attr_array(current, "AXWindows")
                    } else {
                        ax_get_children(current)
                    };

                    if let Some(child_ref) = children.get(child_idx) {
                        let child = *child_ref;
                        CFRetain(child as CFTypeRef);
                        owned.push(child);
                        current = child;
                    } else {
                        ax_release_all(&children);
                        for &e in &owned {
                            ax_release_one(e);
                        }
                        return Ok(None);
                    }
                    ax_release_all(&children);
                }

                let value = ax_get_string(current, "AXValue");
                for &e in &owned {
                    ax_release_one(e);
                }
                Ok(value)
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod non_macos_impl {
    use super::*;

    impl UiInspector {
        pub fn inspect_window(_window_id: u32) -> Result<UiElement> {
            Err(AppError::Accessibility(
                "AXUIElement is only available on macOS".to_string(),
            ))
        }

        pub fn find_elements(
            _window_id: u32,
            _role: Option<&str>,
            _title: Option<&str>,
        ) -> Result<Vec<UiElement>> {
            Err(AppError::Accessibility(
                "AXUIElement is only available on macOS".to_string(),
            ))
        }

        pub fn get_element_value(_element_id: &str) -> Result<Option<String>> {
            Err(AppError::Accessibility(
                "AXUIElement is only available on macOS".to_string(),
            ))
        }
    }
}
