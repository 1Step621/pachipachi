use std::str::FromStr;

use anyhow::{Result, bail, ensure};
use evdev::KeyCode;

#[derive(Clone, Copy)]
pub enum Modifier {
    Shift = 1,
    Ctrl = 2,
    Alt = 4,
    Super = 8,
}

impl Modifier {
    pub fn from_key(key: KeyCode) -> Option<Self> {
        match key {
            KeyCode::KEY_LEFTSHIFT | KeyCode::KEY_RIGHTSHIFT => Some(Self::Shift),
            KeyCode::KEY_LEFTCTRL | KeyCode::KEY_RIGHTCTRL => Some(Self::Ctrl),
            KeyCode::KEY_LEFTALT | KeyCode::KEY_RIGHTALT => Some(Self::Alt),
            KeyCode::KEY_LEFTMETA | KeyCode::KEY_RIGHTMETA => Some(Self::Super),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    pub fn from_keys(keys: impl IntoIterator<Item = KeyCode>) -> Self {
        keys.into_iter()
            .filter_map(Modifier::from_key)
            .fold(Self::default(), |flags, modifier| {
                flags.union(Self(modifier as u8))
            })
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyChord {
    pub fn is_typing(self) -> bool {
        if self.modifiers.0 & (Modifier::Ctrl as u8 | Modifier::Alt as u8 | Modifier::Super as u8)
            != 0
        {
            return false;
        }
        // Accept physical text/editing keys. Navigation, mode switches, and function keys
        // remain available as explicit image bindings, but don't count as typing by default.
        matches!(
            self.key,
            KeyCode::KEY_A
                | KeyCode::KEY_B
                | KeyCode::KEY_C
                | KeyCode::KEY_D
                | KeyCode::KEY_E
                | KeyCode::KEY_F
                | KeyCode::KEY_G
                | KeyCode::KEY_H
                | KeyCode::KEY_I
                | KeyCode::KEY_J
                | KeyCode::KEY_K
                | KeyCode::KEY_L
                | KeyCode::KEY_M
                | KeyCode::KEY_N
                | KeyCode::KEY_O
                | KeyCode::KEY_P
                | KeyCode::KEY_Q
                | KeyCode::KEY_R
                | KeyCode::KEY_S
                | KeyCode::KEY_T
                | KeyCode::KEY_U
                | KeyCode::KEY_V
                | KeyCode::KEY_W
                | KeyCode::KEY_X
                | KeyCode::KEY_Y
                | KeyCode::KEY_Z
                | KeyCode::KEY_0
                | KeyCode::KEY_1
                | KeyCode::KEY_2
                | KeyCode::KEY_3
                | KeyCode::KEY_4
                | KeyCode::KEY_5
                | KeyCode::KEY_6
                | KeyCode::KEY_7
                | KeyCode::KEY_8
                | KeyCode::KEY_9
                | KeyCode::KEY_MINUS
                | KeyCode::KEY_EQUAL
                | KeyCode::KEY_LEFTBRACE
                | KeyCode::KEY_RIGHTBRACE
                | KeyCode::KEY_SEMICOLON
                | KeyCode::KEY_APOSTROPHE
                | KeyCode::KEY_GRAVE
                | KeyCode::KEY_BACKSLASH
                | KeyCode::KEY_COMMA
                | KeyCode::KEY_DOT
                | KeyCode::KEY_SLASH
                | KeyCode::KEY_102ND
                | KeyCode::KEY_RO
                | KeyCode::KEY_YEN
                | KeyCode::KEY_SPACE
                | KeyCode::KEY_ENTER
                | KeyCode::KEY_BACKSPACE
                | KeyCode::KEY_DELETE
                | KeyCode::KEY_KP0
                | KeyCode::KEY_KP1
                | KeyCode::KEY_KP2
                | KeyCode::KEY_KP3
                | KeyCode::KEY_KP4
                | KeyCode::KEY_KP5
                | KeyCode::KEY_KP6
                | KeyCode::KEY_KP7
                | KeyCode::KEY_KP8
                | KeyCode::KEY_KP9
                | KeyCode::KEY_KPPLUS
                | KeyCode::KEY_KPMINUS
                | KeyCode::KEY_KPASTERISK
                | KeyCode::KEY_KPSLASH
                | KeyCode::KEY_KPDOT
                | KeyCode::KEY_KPCOMMA
                | KeyCode::KEY_KPEQUAL
                | KeyCode::KEY_KPENTER
                | KeyCode::KEY_KPPLUSMINUS
        )
    }

    pub fn unmodified(key: KeyCode) -> Self {
        Self {
            key,
            modifiers: Modifiers::default(),
        }
    }
}

impl FromStr for KeyChord {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let mut parts = text.split('+').map(str::trim).collect::<Vec<_>>();
        let name = parts.pop().unwrap_or_default();
        let key = name
            .parse::<KeyCode>()
            .map_err(|_| anyhow::anyhow!("unknown evdev key: {name}"))?;
        ensure!(
            crate::input::is_keyboard_key(key),
            "not a keyboard key: {name}"
        );
        ensure!(
            Modifier::from_key(key).is_none(),
            "modifier-only bindings are unsupported: {name}; use e.g. Shift+KEY_1"
        );
        let mut modifiers = Modifiers::default();
        for part in parts {
            let flag = match part {
                "Shift" => Modifier::Shift,
                "Ctrl" => Modifier::Ctrl,
                "Alt" => Modifier::Alt,
                "Super" => Modifier::Super,
                _ => bail!("unknown modifier: {part}; use Shift, Ctrl, Alt, or Super"),
            } as u8;
            ensure!(modifiers.0 & flag == 0, "duplicate modifier: {part}");
            modifiers.0 |= flag;
        }
        Ok(Self { key, modifiers })
    }
}
