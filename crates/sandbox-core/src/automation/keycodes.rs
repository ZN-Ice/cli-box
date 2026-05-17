/// macOS CGKeyCode values for common keys.
/// Reference: <https://developer.apple.com/documentation/coregraphics/quartz_event_services>
///
/// Convert a key name string to a CGKeyCode
pub fn key_name_to_code(name: &str) -> Option<u16> {
    match name.to_lowercase().as_str() {
        "a" | "key_a" => Some(0x00),
        "s" | "key_s" => Some(0x01),
        "d" | "key_d" => Some(0x02),
        "f" | "key_f" => Some(0x03),
        "h" | "key_h" => Some(0x04),
        "g" | "key_g" => Some(0x05),
        "z" | "key_z" => Some(0x06),
        "x" | "key_x" => Some(0x07),
        "c" | "key_c" => Some(0x08),
        "v" | "key_v" => Some(0x09),
        "b" | "key_b" => Some(0x0B),
        "q" | "key_q" => Some(0x0C),
        "w" | "key_w" => Some(0x0D),
        "e" | "key_e" => Some(0x0E),
        "r" | "key_r" => Some(0x0F),
        "y" | "key_y" => Some(0x10),
        "t" | "key_t" => Some(0x11),
        "1" | "key_1" => Some(0x12),
        "2" | "key_2" => Some(0x13),
        "3" | "key_3" => Some(0x14),
        "4" | "key_4" => Some(0x15),
        "6" | "key_6" => Some(0x16),
        "5" | "key_5" => Some(0x17),
        "=" | "equal" => Some(0x18),
        "9" | "key_9" => Some(0x19),
        "7" | "key_7" => Some(0x1A),
        "-" | "minus" => Some(0x1B),
        "8" | "key_8" => Some(0x1C),
        "0" | "key_0" => Some(0x1D),
        "]" | "right_bracket" => Some(0x1E),
        "o" | "key_o" => Some(0x1F),
        "u" | "key_u" => Some(0x20),
        "[" | "left_bracket" => Some(0x21),
        "i" | "key_i" => Some(0x22),
        "p" | "key_p" => Some(0x23),
        "return" | "enter" => Some(0x24),
        "l" | "key_l" => Some(0x25),
        "j" | "key_j" => Some(0x26),
        "'" | "quote" => Some(0x27),
        "k" | "key_k" => Some(0x28),
        ";" | "semicolon" => Some(0x29),
        "\\" | "backslash" => Some(0x2A),
        "," | "comma" => Some(0x2B),
        "/" | "slash" => Some(0x2C),
        "n" | "key_n" => Some(0x2D),
        "m" | "key_m" => Some(0x2E),
        "." | "period" => Some(0x2F),
        "tab" => Some(0x30),
        "space" => Some(0x31),
        "`" | "backtick" => Some(0x32),
        "delete" | "backspace" => Some(0x33),
        "escape" | "esc" => Some(0x35),
        "command" | "cmd" => Some(0x37),
        "shift" => Some(0x38),
        "caps_lock" => Some(0x39),
        "option" | "alt" => Some(0x3A),
        "control" | "ctrl" => Some(0x3B),
        "right_shift" => Some(0x3C),
        "right_option" | "right_alt" => Some(0x3D),
        "right_control" | "right_ctrl" => Some(0x3E),
        "function" | "fn" => Some(0x3F),
        "f1" => Some(0x7A),
        "f2" => Some(0x7B),
        "f3" => Some(0x7C),
        "f4" => Some(0x7D),
        "f5" => Some(0x7E),
        "f6" => Some(0x7F),
        "f7" => Some(0x80),
        "f8" => Some(0x81),
        "f9" => Some(0x82),
        "f10" => Some(0x83),
        "f11" => Some(0x84),
        "f12" => Some(0x85),
        "home" => Some(0x73),
        "end" => Some(0x77),
        "page_up" => Some(0x74),
        "page_down" => Some(0x79),
        "left" | "left_arrow" => Some(0x7B),
        "right" | "right_arrow" => Some(0x7C),
        "down" | "down_arrow" => Some(0x7D),
        "up" | "up_arrow" => Some(0x7E),
        "volume_up" => Some(0x48),
        "volume_down" => Some(0x49),
        "mute" => Some(0x4A),
        _ => None,
    }
}

/// Get shift state needed for a character
pub fn char_needs_shift(c: char) -> bool {
    matches!(
        c,
        'A'..='Z'
            | '~'
            | '!'
            | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '('
            | ')'
            | '_'
            | '+'
            | '{'
            | '}'
            | '|'
            | ':'
            | '"'
            | '<'
            | '>'
            | '?'
    )
}

/// Get the base key (unshifted) for a character
pub fn char_to_key_name(c: char) -> Option<&'static str> {
    match c.to_ascii_lowercase() {
        'a'..='z' => match c.to_ascii_lowercase() {
            'a' => Some("a"),
            'b' => Some("b"),
            'c' => Some("c"),
            'd' => Some("d"),
            'e' => Some("e"),
            'f' => Some("f"),
            'g' => Some("g"),
            'h' => Some("h"),
            'i' => Some("i"),
            'j' => Some("j"),
            'k' => Some("k"),
            'l' => Some("l"),
            'm' => Some("m"),
            'n' => Some("n"),
            'o' => Some("o"),
            'p' => Some("p"),
            'q' => Some("q"),
            'r' => Some("r"),
            's' => Some("s"),
            't' => Some("t"),
            'u' => Some("u"),
            'v' => Some("v"),
            'w' => Some("w"),
            'x' => Some("x"),
            'y' => Some("y"),
            'z' => Some("z"),
            _ => None,
        },
        '0' => Some("0"),
        '1' => Some("1"),
        '2' => Some("2"),
        '3' => Some("3"),
        '4' => Some("4"),
        '5' => Some("5"),
        '6' => Some("6"),
        '7' => Some("7"),
        '8' => Some("8"),
        '9' => Some("9"),
        ' ' => Some("space"),
        '\n' | '\r' => Some("return"),
        '\t' => Some("tab"),
        '-' => Some("minus"),
        '=' => Some("equal"),
        '[' => Some("left_bracket"),
        ']' => Some("right_bracket"),
        '\\' => Some("backslash"),
        ';' => Some("semicolon"),
        '\'' => Some("quote"),
        ',' => Some("comma"),
        '.' => Some("period"),
        '/' => Some("slash"),
        '`' => Some("backtick"),
        _ => None,
    }
}

/// CGEvent modifier flags
pub mod flags {
    pub const SHIFT: u64 = 1 << 17;
    pub const COMMAND: u64 = 1 << 20;
    pub const ALTERNATE: u64 = 1 << 18;
    pub const CONTROL: u64 = 1 << 19;
    pub const ALPHA_SHIFT: u64 = 1 << 16;
}

/// Convert a modifier name to its CGEvent flag
pub fn modifier_to_flag(name: &str) -> Option<u64> {
    match name.to_lowercase().as_str() {
        "shift" => Some(flags::SHIFT),
        "command" | "cmd" => Some(flags::COMMAND),
        "option" | "alt" => Some(flags::ALTERNATE),
        "control" | "ctrl" => Some(flags::CONTROL),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_name_to_code_common() {
        assert_eq!(key_name_to_code("return"), Some(0x24));
        assert_eq!(key_name_to_code("space"), Some(0x31));
        assert_eq!(key_name_to_code("tab"), Some(0x30));
        assert_eq!(key_name_to_code("escape"), Some(0x35));
        assert_eq!(key_name_to_code("a"), Some(0x00));
    }

    #[test]
    fn test_key_name_case_insensitive() {
        assert_eq!(key_name_to_code("RETURN"), Some(0x24));
        assert_eq!(key_name_to_code("Space"), Some(0x31));
    }

    #[test]
    fn test_char_needs_shift() {
        assert!(char_needs_shift('A'));
        assert!(char_needs_shift('!'));
        assert!(!char_needs_shift('a'));
        assert!(!char_needs_shift('1'));
    }

    #[test]
    fn test_modifier_to_flag() {
        assert_eq!(modifier_to_flag("shift"), Some(flags::SHIFT));
        assert_eq!(modifier_to_flag("cmd"), Some(flags::COMMAND));
        assert_eq!(modifier_to_flag("ctrl"), Some(flags::CONTROL));
        assert_eq!(modifier_to_flag("unknown"), None);
    }

    #[test]
    fn test_key_name_modifier_keys() {
        assert_eq!(key_name_to_code("command"), Some(0x37));
        assert_eq!(key_name_to_code("cmd"), Some(0x37));
        assert_eq!(key_name_to_code("shift"), Some(0x38));
        assert_eq!(key_name_to_code("caps_lock"), Some(0x39));
        assert_eq!(key_name_to_code("option"), Some(0x3A));
        assert_eq!(key_name_to_code("alt"), Some(0x3A));
        assert_eq!(key_name_to_code("control"), Some(0x3B));
        assert_eq!(key_name_to_code("ctrl"), Some(0x3B));
        assert_eq!(key_name_to_code("right_shift"), Some(0x3C));
        assert_eq!(key_name_to_code("right_option"), Some(0x3D));
        assert_eq!(key_name_to_code("right_alt"), Some(0x3D));
        assert_eq!(key_name_to_code("right_control"), Some(0x3E));
        assert_eq!(key_name_to_code("right_ctrl"), Some(0x3E));
        assert_eq!(key_name_to_code("function"), Some(0x3F));
        assert_eq!(key_name_to_code("fn"), Some(0x3F));
    }

    #[test]
    fn test_key_name_function_keys() {
        assert_eq!(key_name_to_code("f1"), Some(0x7A));
        assert_eq!(key_name_to_code("f2"), Some(0x7B));
        assert_eq!(key_name_to_code("f3"), Some(0x7C));
        assert_eq!(key_name_to_code("f4"), Some(0x7D));
        assert_eq!(key_name_to_code("f5"), Some(0x7E));
        assert_eq!(key_name_to_code("f6"), Some(0x7F));
        assert_eq!(key_name_to_code("f7"), Some(0x80));
        assert_eq!(key_name_to_code("f8"), Some(0x81));
        assert_eq!(key_name_to_code("f9"), Some(0x82));
        assert_eq!(key_name_to_code("f10"), Some(0x83));
        assert_eq!(key_name_to_code("f11"), Some(0x84));
        assert_eq!(key_name_to_code("f12"), Some(0x85));
    }

    #[test]
    fn test_key_name_navigation() {
        assert_eq!(key_name_to_code("home"), Some(0x73));
        assert_eq!(key_name_to_code("end"), Some(0x77));
        assert_eq!(key_name_to_code("page_up"), Some(0x74));
        assert_eq!(key_name_to_code("page_down"), Some(0x79));
        assert_eq!(key_name_to_code("left"), Some(0x7B));
        assert_eq!(key_name_to_code("right"), Some(0x7C));
        assert_eq!(key_name_to_code("down"), Some(0x7D));
        assert_eq!(key_name_to_code("up"), Some(0x7E));
        assert_eq!(key_name_to_code("left_arrow"), Some(0x7B));
        assert_eq!(key_name_to_code("right_arrow"), Some(0x7C));
        assert_eq!(key_name_to_code("down_arrow"), Some(0x7D));
        assert_eq!(key_name_to_code("up_arrow"), Some(0x7E));
    }

    #[test]
    fn test_key_name_media() {
        assert_eq!(key_name_to_code("volume_up"), Some(0x48));
        assert_eq!(key_name_to_code("volume_down"), Some(0x49));
        assert_eq!(key_name_to_code("mute"), Some(0x4A));
    }

    #[test]
    fn test_key_name_special() {
        assert_eq!(key_name_to_code("delete"), Some(0x33));
        assert_eq!(key_name_to_code("backspace"), Some(0x33));
        assert_eq!(key_name_to_code("backtick"), Some(0x32));
    }

    #[test]
    fn test_key_name_unknown() {
        assert_eq!(key_name_to_code("nonexistent"), None);
        assert_eq!(key_name_to_code(""), None);
    }

    #[test]
    fn test_char_to_key_name_letters() {
        assert_eq!(char_to_key_name('a'), Some("a"));
        assert_eq!(char_to_key_name('z'), Some("z"));
        assert_eq!(char_to_key_name('A'), Some("a"));
        assert_eq!(char_to_key_name('Z'), Some("z"));
    }

    #[test]
    fn test_char_to_key_name_digits() {
        for c in '0'..='9' {
            assert!(char_to_key_name(c).is_some(), "char {c}");
        }
        assert_eq!(char_to_key_name('5'), Some("5"));
    }

    #[test]
    fn test_char_to_key_name_special() {
        assert_eq!(char_to_key_name(' '), Some("space"));
        assert_eq!(char_to_key_name('\n'), Some("return"));
        assert_eq!(char_to_key_name('\r'), Some("return"));
        assert_eq!(char_to_key_name('\t'), Some("tab"));
        assert_eq!(char_to_key_name('-'), Some("minus"));
        assert_eq!(char_to_key_name('='), Some("equal"));
        assert_eq!(char_to_key_name('['), Some("left_bracket"));
        assert_eq!(char_to_key_name(']'), Some("right_bracket"));
        assert_eq!(char_to_key_name('\\'), Some("backslash"));
        assert_eq!(char_to_key_name(';'), Some("semicolon"));
        assert_eq!(char_to_key_name('\''), Some("quote"));
        assert_eq!(char_to_key_name(','), Some("comma"));
        assert_eq!(char_to_key_name('.'), Some("period"));
        assert_eq!(char_to_key_name('/'), Some("slash"));
        assert_eq!(char_to_key_name('`'), Some("backtick"));
    }

    #[test]
    fn test_char_to_key_name_unknown() {
        assert_eq!(char_to_key_name('!'), None);
        assert_eq!(char_to_key_name('@'), None);
        assert_eq!(char_to_key_name('~'), None);
    }

    #[test]
    fn test_char_needs_shift_comprehensive() {
        let shift_chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ!@#$%^&*()_+{}|:\"<>?";
        for c in shift_chars.chars() {
            assert!(char_needs_shift(c), "char '{c}' should need shift");
        }
        let no_shift = "abcdefghijklmnopqrstuvwxyz0123456789-=[]\\;',./`";
        for c in no_shift.chars() {
            assert!(!char_needs_shift(c), "char '{c}' should NOT need shift");
        }
    }

    #[test]
    fn test_modifier_to_flag_aliases() {
        assert_eq!(modifier_to_flag("command"), Some(flags::COMMAND));
        assert_eq!(modifier_to_flag("alt"), Some(flags::ALTERNATE));
    }
}
