use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail, ensure};
use serde::Deserialize;

use crate::keys::KeyChord;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub position: Position,
    pub images: Images,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Position {
    pub anchor: Corner,
    pub x: u16,
    pub y: u16,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            anchor: Corner::BottomRight,
            x: 24,
            y: 24,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Images {
    pub odd: PathBuf,
    pub even: PathBuf,
    pub neutral: PathBuf,
    #[serde(default = "default_size")]
    pub width: u16,
    #[serde(default = "default_size")]
    pub height: u16,
    #[serde(default, deserialize_with = "deserialize_keys")]
    pub keys: HashMap<KeyChord, PathBuf>,
}

fn default_size() -> u16 {
    160
}

fn deserialize_keys<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<KeyChord, PathBuf>, D::Error> {
    let entries = HashMap::<String, PathBuf>::deserialize(deserializer)?;
    let mut chords = HashMap::new();
    for (name, path) in entries {
        let chord = name.parse::<KeyChord>().map_err(serde::de::Error::custom)?;
        if chords.insert(chord, path).is_some() {
            return Err(serde::de::Error::custom(format!(
                "duplicate key combination: {name}"
            )));
        }
    }
    Ok(chords)
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| {
            format!(
                "cannot read config {} (see config.example.toml)",
                path.display()
            )
        })?;
        Self::parse(&text, path.parent().unwrap_or(Path::new(".")))
            .with_context(|| format!("invalid config {}", path.display()))
    }

    fn parse(text: &str, directory: &Path) -> Result<Self> {
        let mut config: Self = toml::from_str(text)?;
        ensure!(
            (1..=4096).contains(&config.images.width),
            "images.width must be between 1 and 4096"
        );
        ensure!(
            (1..=4096).contains(&config.images.height),
            "images.height must be between 1 and 4096"
        );
        for path in [
            &mut config.images.odd,
            &mut config.images.even,
            &mut config.images.neutral,
        ]
        .into_iter()
        .chain(config.images.keys.values_mut())
        {
            ensure!(
                !path.as_os_str().is_empty(),
                "image paths must not be empty"
            );
            *path = resolve_image_path(path, directory)?;
        }
        Ok(config)
    }
}

fn resolve_image_path(path: &Path, directory: &Path) -> Result<PathBuf> {
    if path == Path::new("~") || path.starts_with("~/") {
        let home = env::var_os("HOME").context("HOME is not set; cannot expand ~")?;
        return Ok(PathBuf::from(home).join(path.strip_prefix("~")?));
    }
    if path.to_string_lossy().starts_with('~') {
        bail!("~user paths are unsupported; use an absolute path or ~/ instead");
    }
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        directory.join(path)
    })
}

pub fn default_path() -> Result<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context("set XDG_CONFIG_HOME, HOME, or pass --config")?;
    Ok(base.join("pachipachi/config.toml"))
}
