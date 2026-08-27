use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use uic::assets::LucideAssets;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/*.svg"]
pub struct AppAssets;

impl AppAssets {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AppAssets {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(file) = Self::get(path) {
            Ok(Some(file.data))
        } else {
            LucideAssets::new().load(path)
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|item| item.starts_with(path).then(|| item.into()))
            .chain(LucideAssets::new().list(path)?)
            .collect())
    }
}
