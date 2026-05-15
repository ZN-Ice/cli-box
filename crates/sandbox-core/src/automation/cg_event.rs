use crate::automation::keycodes;
use crate::error::{AppError, Result};

/// Mouse button type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Input simulator using CGEvents (macOS Core Graphics)
pub struct InputSimulator;

impl InputSimulator {
    /// Simulate a mouse click at the given coordinates
    #[cfg(target_os = "macos")]
    pub fn click(x: f64, y: f64, button: MouseButton) -> Result<()> {
        use core_graphics::event::CGEventType;
        use core_graphics::event_source::CGEventSource;

        let source = CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState,
        )
        .map_err(|e| AppError::Input(format!("Failed to create event source: {:?}", e)))?;

        let position = core_graphics::geometry::CGPoint::new(x, y);
        let (down_type, up_type) = match button {
            MouseButton::Left => (CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
            MouseButton::Right => (CGEventType::RightMouseDown, CGEventType::RightMouseUp),
            MouseButton::Middle => (CGEventType::OtherMouseDown, CGEventType::OtherMouseUp),
        };

        mouse_event(&source, down_type, position, button)?;
        mouse_event(&source, up_type, position, button)?;

        tracing::debug!("Click at ({}, {}), button={:?}", x, y, button);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn click(_x: f64, _y: f64, _button: MouseButton) -> Result<()> {
        Err(AppError::Input("click only supported on macOS".into()))
    }

    /// Simulate a double click at the given coordinates
    pub fn double_click(x: f64, y: f64) -> Result<()> {
        // Set double-click interval via CGEvent
        #[cfg(target_os = "macos")]
        {
            // CGEventSetIntegerValueField for click count
            Self::click(x, y, MouseButton::Left)?;
            std::thread::sleep(std::time::Duration::from_millis(50));
            Self::click(x, y, MouseButton::Left)?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (x, y);
            return Err(AppError::Input(
                "double_click only supported on macOS".into(),
            ));
        }
        Ok(())
    }

    /// Simulate typing text character by character
    #[cfg(target_os = "macos")]
    pub fn type_text(text: &str) -> Result<()> {
        for c in text.chars() {
            type_character(c)?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn type_text(_text: &str) -> Result<()> {
        Err(AppError::Input("type_text only supported on macOS".into()))
    }

    /// Simulate pressing a key with optional modifiers
    #[cfg(target_os = "macos")]
    pub fn press_key(key: &str, modifiers: &[&str]) -> Result<()> {
        use core_graphics::event::CGEventFlags;
        use core_graphics::event::{CGEvent, CGEventTapLocation};
        use core_graphics::event_source::CGEventSource;

        let key_code = keycodes::key_name_to_code(key)
            .ok_or_else(|| AppError::Input(format!("Unknown key: {}", key)))?;

        let source = CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState,
        )
        .map_err(|e| AppError::Input(format!("Failed to create event source: {:?}", e)))?;

        // Build modifier flags
        let mut flags = 0u64;
        for m in modifiers {
            if let Some(flag) = keycodes::modifier_to_flag(m) {
                flags |= flag;
            }
        }

        let key_down = CGEvent::new_keyboard_event(source.clone(), key_code, true)
            .map_err(|e| AppError::Input(format!("Failed to create key-down event: {:?}", e)))?;
        if flags > 0 {
            key_down.set_flags(CGEventFlags::from_bits_truncate(flags));
        }
        key_down.post(CGEventTapLocation::HID);

        let key_up = CGEvent::new_keyboard_event(source, key_code, false)
            .map_err(|e| AppError::Input(format!("Failed to create key-up event: {:?}", e)))?;
        if flags > 0 {
            key_up.set_flags(CGEventFlags::from_bits_truncate(flags));
        }
        key_up.post(CGEventTapLocation::HID);

        tracing::debug!("Press key={}, modifiers={:?}", key, modifiers);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn press_key(_key: &str, _modifiers: &[&str]) -> Result<()> {
        Err(AppError::Input("press_key only supported on macOS".into()))
    }

    /// Simulate scrolling
    #[cfg(target_os = "macos")]
    pub fn scroll(x: f64, y: f64, direction: &str, amount: i32) -> Result<()> {
        use core_graphics::event::{CGEvent, CGEventTapLocation, ScrollEventUnit};
        use core_graphics::event_source::CGEventSource;

        let _ = (x, y);
        let source = CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState,
        )
        .map_err(|e| AppError::Input(format!("Failed to create event source: {:?}", e)))?;

        let (delta_y, delta_x): (i32, i32) = match direction.to_lowercase().as_str() {
            "up" => (-amount, 0),
            "down" => (amount, 0),
            "left" => (0, -amount),
            "right" => (0, amount),
            _ => {
                return Err(AppError::Input(format!(
                    "Unknown scroll direction: {}",
                    direction
                )))
            }
        };

        let scroll =
            CGEvent::new_scroll_event(source, ScrollEventUnit::LINE, 2, delta_y, delta_x, 0)
                .map_err(|e| AppError::Input(format!("Failed to create scroll event: {:?}", e)))?;

        scroll.post(CGEventTapLocation::HID);
        tracing::debug!("Scroll dir={}, amount={}", direction, amount);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn scroll(_x: f64, _y: f64, _direction: &str, _amount: i32) -> Result<()> {
        Err(AppError::Input("scroll only supported on macOS".into()))
    }

    /// Simulate a drag from one point to another
    #[cfg(target_os = "macos")]
    pub fn drag(from_x: f64, from_y: f64, to_x: f64, to_y: f64) -> Result<()> {
        use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType};
        use core_graphics::event_source::CGEventSource;
        use core_graphics::geometry::CGPoint;

        let source = CGEventSource::new(
            core_graphics::event_source::CGEventSourceStateID::CombinedSessionState,
        )
        .map_err(|e| AppError::Input(format!("Failed to create event source: {:?}", e)))?;

        let start = CGPoint::new(from_x, from_y);
        let end = CGPoint::new(to_x, to_y);

        // Mouse down at start
        let down = CGEvent::new_mouse_event(
            source.clone(),
            CGEventType::LeftMouseDown,
            start,
            core_graphics::event::CGMouseButton::Left,
        )
        .map_err(|e| AppError::Input(format!("Failed to create mouse-down event: {:?}", e)))?;
        down.post(CGEventTapLocation::HID);

        // Drag to end (small steps for smoothness)
        let steps = 20;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let point = CGPoint::new(
                start.x + (end.x - start.x) * t,
                start.y + (end.y - start.y) * t,
            );
            let drag = CGEvent::new_mouse_event(
                source.clone(),
                CGEventType::LeftMouseDragged,
                point,
                core_graphics::event::CGMouseButton::Left,
            )
            .map_err(|e| AppError::Input(format!("Failed to create drag event: {:?}", e)))?;
            drag.post(CGEventTapLocation::HID);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        // Mouse up at end
        let up = CGEvent::new_mouse_event(
            source,
            CGEventType::LeftMouseUp,
            end,
            core_graphics::event::CGMouseButton::Left,
        )
        .map_err(|e| AppError::Input(format!("Failed to create mouse-up event: {:?}", e)))?;
        up.post(CGEventTapLocation::HID);

        tracing::debug!("Drag from ({},{}) to ({},{})", from_x, from_y, to_x, to_y);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn drag(_from_x: f64, _from_y: f64, _to_x: f64, _to_y: f64) -> Result<()> {
        Err(AppError::Input("drag only supported on macOS".into()))
    }
}

/// Helper: create and post a mouse event
#[cfg(target_os = "macos")]
fn mouse_event(
    source: &core_graphics::event_source::CGEventSource,
    event_type: core_graphics::event::CGEventType,
    position: core_graphics::geometry::CGPoint,
    button: MouseButton,
) -> Result<()> {
    use core_graphics::event::{CGEvent, CGEventTapLocation};

    let cg_button = match button {
        MouseButton::Left => core_graphics::event::CGMouseButton::Left,
        MouseButton::Right => core_graphics::event::CGMouseButton::Right,
        MouseButton::Middle => core_graphics::event::CGMouseButton::Center,
    };

    let event = CGEvent::new_mouse_event(source.clone(), event_type, position, cg_button)
        .map_err(|e| AppError::Input(format!("Failed to create mouse event: {:?}", e)))?;

    event.post(CGEventTapLocation::HID);
    Ok(())
}

/// Type a single character using CGEvent keyboard simulation
#[cfg(target_os = "macos")]
fn type_character(c: char) -> Result<()> {
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::CGEventSource;

    let needs_shift = keycodes::char_needs_shift(c);
    let key_name = keycodes::char_to_key_name(c)
        .ok_or_else(|| AppError::Input(format!("Cannot type character: '{}'", c)))?;
    let key_code = keycodes::key_name_to_code(key_name)
        .ok_or_else(|| AppError::Input(format!("No keycode for: '{}'", key_name)))?;

    let source =
        CGEventSource::new(core_graphics::event_source::CGEventSourceStateID::CombinedSessionState)
            .map_err(|e| AppError::Input(format!("Failed to create event source: {:?}", e)))?;

    // Press shift if needed
    if needs_shift {
        let shift_down = CGEvent::new_keyboard_event(source.clone(), 0x38, true)
            .map_err(|e| AppError::Input(format!("Failed to create shift-down event: {:?}", e)))?;
        shift_down.post(CGEventTapLocation::HID);
    }

    // Key down
    let key_down = CGEvent::new_keyboard_event(source.clone(), key_code, true)
        .map_err(|e| AppError::Input(format!("Failed to create key-down for '{}': {:?}", c, e)))?;
    if needs_shift {
        use core_graphics::event::CGEventFlags;
        key_down.set_flags(CGEventFlags::CGEventFlagShift);
    }
    key_down.post(CGEventTapLocation::HID);

    // Key up
    let key_up = CGEvent::new_keyboard_event(source.clone(), key_code, false)
        .map_err(|e| AppError::Input(format!("Failed to create key-up for '{}': {:?}", c, e)))?;
    key_up.post(CGEventTapLocation::HID);

    // Release shift if needed
    if needs_shift {
        let shift_up = CGEvent::new_keyboard_event(source, 0x38, false)
            .map_err(|e| AppError::Input(format!("Failed to create shift-up event: {:?}", e)))?;
        shift_up.post(CGEventTapLocation::HID);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_button_display() {
        assert_eq!(format!("{:?}", MouseButton::Left), "Left");
        assert_eq!(format!("{:?}", MouseButton::Right), "Right");
    }
}
