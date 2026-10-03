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
