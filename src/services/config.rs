use anyhow::{Context, Result, bail};
use gpui::SharedString;
use serde::Deserialize;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG: &str = include_str!("../../config.toml.example");

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClientConfig {
    pub logging: LoggingConfig,
    pub fonts: FontsConfig,
}

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct FontsConfig {
    pub font_family: Option<SharedString>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub directory: PathBuf,
    pub filter: String,
    pub max_file_size_bytes: u64,
    pub retained_files: usize,
    pub console: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("~/.local/state/romus-demo/logs"),
            filter: "info,gpui::window=off,wgpu=error,blade_graphics=error,naga=error,zbus=error,sum_tree=error,tracing::span=error,gpui_media::startup=debug,gpui_media::decv_frame=debug,gpui_media::frame_drop=debug".to_owned(),
            max_file_size_bytes: 10 * 1024 * 1024,
            retained_files: 7,
            console: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoadedClientConfig {
    pub path: PathBuf,
    pub config: ClientConfig,
}

impl LoadedClientConfig {
    pub fn load_default() -> Result<Self> {
        Self::load_or_create(default_config_path()?)
    }

    pub fn load_or_create(path: PathBuf) -> Result<Self> {
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                create_default_config(&path)?;
                fs::read_to_string(&path)
                    .with_context(|| format!("failed to read client config {}", path.display()))?
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read client config {}", path.display()));
            }
        };
        let config: ClientConfig = toml::from_str(&contents)
            .with_context(|| format!("invalid client config {}", path.display()))?;
        config.validate()?;
        Ok(Self { path, config })
    }

    pub fn log_directory(&self) -> Result<PathBuf> {
        let directory = expand_home(&self.config.logging.directory)?;
        if directory.is_absolute() {
            return Ok(directory);
        }
        let parent = self
            .path
            .parent()
            .context("client config path has no parent directory")?;
        Ok(parent.join(directory))
    }
}

impl gpui::Global for LoadedClientConfig {}

impl ClientConfig {
    fn validate(&self) -> Result<()> {
        if self.logging.directory.as_os_str().is_empty() {
            bail!("logging.directory must not be empty");
        }
        if self.logging.filter.trim().is_empty() {
            bail!("logging.filter must not be empty");
        }
        if self.logging.max_file_size_bytes == 0 {
            bail!("logging.max_file_size_bytes must be greater than zero");
        }
        if self.logging.retained_files == 0 {
            bail!("logging.retained_files must be greater than zero");
        }
        Ok(())
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("unable to determine the user's home directory")?;
    Ok(home.join(".config/romus-demo-local-player/config.toml"))
}

fn create_default_config(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .context("client config path has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file
            .write_all(DEFAULT_CONFIG.as_bytes())
            .with_context(|| format!("failed to write default client config {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to create client config {}", path.display()))
        }
    }
}

fn expand_home(path: &Path) -> Result<PathBuf> {
    let mut components = path.components();
    let Some(first) = components.next() else {
        return Ok(path.to_owned());
    };
    if first.as_os_str() != OsStr::new("~") {
        return Ok(path.to_owned());
    }
    let home = dirs::home_dir().context("unable to determine the user's home directory")?;
    Ok(home.join(components.as_path()))
}
