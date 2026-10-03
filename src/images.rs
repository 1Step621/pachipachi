use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, Result};
use gpui::RenderImage;

use crate::{config::Images, keys::KeyChord};

pub struct PetImages {
    odd: Arc<RenderImage>,
    even: Arc<RenderImage>,
    keys: HashMap<KeyChord, Arc<RenderImage>>,
}

impl PetImages {
    pub fn load(config: &Images) -> Result<Self> {
        // Decode all images before opening a window; invalid files fail with their paths.
        let mut cache = HashMap::<PathBuf, Arc<RenderImage>>::new();
        let mut load = |path: &Path| -> Result<Arc<RenderImage>> {
            if let Some(image) = cache.get(path) {
                return Ok(image.clone());
            }
            let mut pixels = image::ImageReader::open(path)
                .with_context(|| format!("cannot open image {}", path.display()))?
                .with_guessed_format()?
                .decode()
                .with_context(|| {
                    format!(
                        "cannot decode image {} (supported: PNG, JPEG, WebP)",
                        path.display()
                    )
                })?
                .into_rgba8();
            // GPUI's RenderImage expects BGRA, with straight alpha.
            for pixel in pixels.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            let image = Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]));
            cache.insert(path.to_owned(), image.clone());
            Ok(image)
        };
        let odd = load(&config.odd)?;
        let even = load(&config.even)?;
        let keys = config
            .keys
            .iter()
            .map(|(key, path)| Ok((*key, load(path)?)))
            .collect::<Result<_>>()?;
        Ok(Self { odd, even, keys })
    }

    pub fn select(&self, key: Option<KeyChord>, odd: bool) -> Arc<RenderImage> {
        key.and_then(|key| self.keys.get(&key))
            .unwrap_or(if odd { &self.odd } else { &self.even })
            .clone()
    }
}
