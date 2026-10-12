//! Maps crossterm input events to the browser's neutral keys and mouse events.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent as TermMouse,
    MouseEventKind,
};

use browser::{Button, MouseEvent, MouseKind};
use viewer::{Key, KeyPress, Mods};

fn mods(m: KeyModifiers) -> Mods {
    Mods {
        ctrl: m.contains(KeyModifiers::CONTROL),
        alt: m.contains(KeyModifiers::ALT),
        shift: m.contains(KeyModifiers::SHIFT),
    }
}

/// `None` for key releases and keys the browser has no use for.
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
        KeyCode::F(n) if (1..=12).contains(&n) => Key::F(n),
        _ => return None,
    };
    Some(KeyPress::new(key, mods(k.modifiers)))
}

/// Clicks and the wheel; moves, drags and releases are not used.
pub fn map_mouse(m: TermMouse) -> Option<MouseEvent> {
    let kind = match m.kind {
        MouseEventKind::Down(MouseButton::Left) => MouseKind::Down(Button::Left),
        MouseEventKind::Down(MouseButton::Middle) => MouseKind::Down(Button::Middle),
        MouseEventKind::Down(MouseButton::Right) => MouseKind::Down(Button::Right),
        MouseEventKind::ScrollDown => MouseKind::Wheel(1),
        MouseEventKind::ScrollUp => MouseKind::Wheel(-1),
        _ => return None,
    };
    Some(MouseEvent {
        col: m.column as usize,
        row: m.row as usize,
        kind,
        mods: mods(m.modifiers),
    })
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
        assert!(alt.mods.alt && !alt.mods.ctrl && !alt.mods.shift);
        assert_eq!(alt.key, Key::Left);
        let shift = map(ev(KeyCode::Enter, KeyModifiers::SHIFT)).unwrap();
        assert!(shift.mods.shift);
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
            (KeyCode::F(1), Key::F(1)),
            (KeyCode::F(12), Key::F(12)),
        ] {
            assert_eq!(map(ev(c, KeyModifiers::NONE)), Some(KeyPress::plain(k)));
        }
    }

    #[test]
    fn releases_and_unknown_keys_are_dropped() {
        let mut k = ev(KeyCode::Char('j'), KeyModifiers::NONE);
        k.kind = KeyEventKind::Release;
        assert_eq!(map(k), None);
        assert_eq!(map(ev(KeyCode::F(13), KeyModifiers::NONE)), None);
        assert_eq!(map(ev(KeyCode::Insert, KeyModifiers::NONE)), None);
    }

    #[test]
    fn mouse_clicks_and_wheel() {
        let m = |kind, modifiers| TermMouse {
            kind,
            column: 7,
            row: 3,
            modifiers,
        };
        let e = map_mouse(m(
            MouseEventKind::Down(MouseButton::Left),
            KeyModifiers::CONTROL,
        ))
        .unwrap();
        assert_eq!((e.col, e.row), (7, 3));
        assert_eq!(e.kind, MouseKind::Down(Button::Left));
        assert!(e.mods.ctrl && !e.mods.shift);
        let e = map_mouse(m(
            MouseEventKind::Down(MouseButton::Middle),
            KeyModifiers::NONE,
        ))
        .unwrap();
        assert_eq!(e.kind, MouseKind::Down(Button::Middle));
        assert_eq!(
            map_mouse(m(MouseEventKind::ScrollDown, KeyModifiers::NONE))
                .unwrap()
                .kind,
            MouseKind::Wheel(1)
        );
        assert_eq!(
            map_mouse(m(MouseEventKind::ScrollUp, KeyModifiers::NONE))
                .unwrap()
                .kind,
            MouseKind::Wheel(-1)
        );
        assert!(map_mouse(m(MouseEventKind::Moved, KeyModifiers::NONE)).is_none());
        assert!(map_mouse(m(MouseEventKind::Up(MouseButton::Left), KeyModifiers::NONE)).is_none());
    }
}
