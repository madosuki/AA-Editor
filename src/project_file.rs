use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const CURRENT_PROJECT_FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub items: Vec<ProjectItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectItem {
    pub text: String,
}

impl ProjectFile {
    pub fn from_texts(texts: Vec<String>) -> Self {
        Self {
            version: CURRENT_PROJECT_FILE_VERSION,
            items: texts.into_iter().map(|text| ProjectItem { text }).collect(),
        }
    }

    pub fn to_texts(&self) -> Vec<String> {
        self.items.iter().map(|item| item.text.clone()).collect()
    }

    pub fn read_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let json = fs::read_to_string(path)
            .with_context(|| format!("failed to read project file: {}", path.display()))?;

        serde_json::from_str(&json)
            .with_context(|| format!("failed to parse project file: {}", path.display()))
    }

    pub fn write_to_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let json =
            serde_json::to_string_pretty(self).context("failed to serialize project file")?;

        fs::write(path, json)
            .with_context(|| format!("failed to write project file: {}", path.display()))
    }
}
