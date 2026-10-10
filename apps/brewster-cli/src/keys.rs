//! Maps crossterm key events to the viewer's neutral keys.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use viewer::{Key, KeyPress, Mods};

/// `None` for key releases and keys the viewer has no use for.
pub fn map(k: KeyEvent) -> Option<KeyPress> {
    if k.kind == KeyEventKind::Release {
        return None;
    }
    let key = match k.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Esc => Key::Esc,
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        _ => return None,
    };
    Some(KeyPress::new(
        key,
        Mods {
            ctrl: k.modifiers.contains(KeyModifiers::CONTROL),
            alt: k.modifiers.contains(KeyModifiers::ALT),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    #[test]
    fn plain_and_modified_keys() {
        assert_eq!(
            map(ev(KeyCode::Char('j'), KeyModifiers::NONE)),
            Some(KeyPress::plain(Key::Char('j')))
        );
        assert_eq!(
            map(ev(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(KeyPress::ctrl(Key::Char('c')))
        );
        let alt = map(ev(KeyCode::Left, KeyModifiers::ALT)).unwrap();
        assert!(alt.mods.alt && !alt.mods.ctrl);
        assert_eq!(alt.key, Key::Left);
    }

    #[test]
    fn named_keys() {
        for (c, k) in [
            (KeyCode::Esc, Key::Esc),
            (KeyCode::Enter, Key::Enter),
            (KeyCode::Tab, Key::Tab),
            (KeyCode::BackTab, Key::BackTab),
            (KeyCode::PageUp, Key::PageUp),
            (KeyCode::PageDown, Key::PageDown),
            (KeyCode::Backspace, Key::Backspace),
            (KeyCode::Delete, Key::Delete),
        ] {
            assert_eq!(map(ev(c, KeyModifiers::NONE)), Some(KeyPress::plain(k)));
        }
    }

    #[test]
    fn releases_and_unknown_keys_are_dropped() {
        let mut k = ev(KeyCode::Char('j'), KeyModifiers::NONE);
        k.kind = KeyEventKind::Release;
        assert_eq!(map(k), None);
        assert_eq!(map(ev(KeyCode::F(5), KeyModifiers::NONE)), None);
    }
}
