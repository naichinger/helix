use gpui::{Keystroke, Modifiers};
use helix_view::input::{KeyCode, KeyEvent, KeyModifiers};

/// Translate platform UTF-16 ranges without splitting a surrogate pair.
pub fn utf16_range(text: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let offset = |target, round_up| {
        let mut units = 0;
        for (byte, ch) in text.char_indices() {
            if target <= units {
                return byte;
            }
            units += ch.len_utf16();
            if target < units {
                return if round_up { byte + ch.len_utf8() } else { byte };
            }
        }
        text.len()
    };
    let start = offset(range.start, false);
    start..offset(range.end, true).max(start)
}

pub fn modifiers(value: Modifiers) -> KeyModifiers {
    let mut result = KeyModifiers::empty();
    result.set(KeyModifiers::SHIFT, value.shift);
    result.set(KeyModifiers::CONTROL, value.control);
    result.set(KeyModifiers::ALT, value.alt);
    result.set(KeyModifiers::SUPER, value.platform);
    result
}

/// GPUI already resolves the keyboard layout and shifted printable characters.
/// Helix represents uppercase characters without an additional SHIFT modifier.
pub fn key(stroke: &Keystroke) -> Option<KeyEvent> {
    let mut mods = modifiers(stroke.modifiers);
    let code = match stroke.key.as_str() {
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "enter" => KeyCode::Enter,
        "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "insert" => KeyCode::Insert,
        name if name.starts_with('f') && name.len() > 1 => KeyCode::F(name[1..].parse().ok()?),
        text => {
            let mut chars = text.chars();
            let mut ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            if mods.contains(KeyModifiers::SHIFT) && ch.is_ascii_lowercase() {
                ch = ch.to_ascii_uppercase();
            }
            mods.remove(KeyModifiers::SHIFT);
            KeyCode::Char(ch)
        }
    };
    Some(KeyEvent {
        code,
        modifiers: mods,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_ranges_preserve_surrogate_pairs() {
        assert_eq!(utf16_range("a😀b", 1..3), 1..5);
        assert_eq!(utf16_range("a😀b", 2..3), 1..5);
        assert_eq!(utf16_range("a😀b", 3..4), 5..6);
        assert_eq!(utf16_range("a😀b", 99..100), 6..6);
    }
    #[test]
    fn keys_preserve_helix_semantics() {
        for (gpui, helix) in [
            ("shift-g", "G"),
            ("ctrl-w", "C-w"),
            ("alt-x", "A-x"),
            ("shift-tab", "S-tab"),
            ("escape", "esc"),
            ("f12", "F12"),
            ("space", "space"),
        ] {
            assert_eq!(
                key(&Keystroke::parse(gpui).unwrap()).unwrap(),
                helix.parse().unwrap(),
                "{gpui}"
            );
        }
    }
}
