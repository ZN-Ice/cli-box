use sandbox_core::automation::cg_event::MouseButton;

#[test]
fn mouse_button_debug_format() {
    assert_eq!(format!("{:?}", MouseButton::Left), "Left");
    assert_eq!(format!("{:?}", MouseButton::Right), "Right");
    assert_eq!(format!("{:?}", MouseButton::Middle), "Middle");
}

#[test]
fn mouse_button_equality() {
    assert_eq!(MouseButton::Left, MouseButton::Left);
    assert_ne!(MouseButton::Left, MouseButton::Right);
    assert_ne!(MouseButton::Right, MouseButton::Middle);
}

#[test]
fn mouse_button_clone() {
    let b = MouseButton::Left;
    let b2 = b; // Copy (MouseButton is Copy)
    assert_eq!(b, b2);
}

#[test]
fn mouse_button_copy() {
    let b = MouseButton::Right;
    let b2 = b;
    assert_eq!(b, b2);
}
