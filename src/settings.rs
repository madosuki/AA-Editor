use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

const CURRENT_SETTINGS_VERSION: &str = "0.0.1";
const APP_CONFIG_DIR_NAME: &str = "aa_editor";
const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_SETTINGS_VERSION.to_string(),
        }
    }
}

impl Settings {
    pub fn load_or_create() -> Result<Self> {
        let path = Self::path()?;
        if path.exists() {
            let json = fs::read_to_string(&path)
                .with_context(|| format!("failed to read settings file: {}", path.display()))?;

            return serde_json::from_str(&json)
                .with_context(|| format!("failed to parse settings file: {}", path.display()));
        }

        let settings = Self::default();
        settings.write()?;
        Ok(settings)
    }

    fn write(&self) -> Result<()> {
        let path = Self::path()?;
        let Some(config_dir) = path.parent() else {
            return Err(anyhow!("failed to resolve settings directory"));
        };

        fs::create_dir_all(config_dir).with_context(|| {
            format!(
                "failed to create settings directory: {}",
                config_dir.display()
            )
        })?;

        let json = serde_json::to_string_pretty(self).context("failed to serialize settings")?;
        fs::write(&path, json)
            .with_context(|| format!("failed to write settings file: {}", path.display()))
    }

    fn path() -> Result<PathBuf> {
        Ok(Self::config_home()?
            .join(APP_CONFIG_DIR_NAME)
            .join(SETTINGS_FILE_NAME))
    }

    fn config_home() -> Result<PathBuf> {
        if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
            return Ok(PathBuf::from(path));
        }

        let Some(home) = env::var_os("HOME").filter(|path| !path.is_empty()) else {
            return Err(anyhow!(
                "XDG_CONFIG_HOME is not set and HOME is unavailable"
            ));
        };

        Ok(PathBuf::from(home).join(".config"))
    }
}
