use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const CURRENT_PROJECT_FILE_VERSION: &str = "0.0.1";
const DEFAULT_MLT_COLLECTION_DIRECTORY_PATH: &str = "~/.local/share/aa_editor/mlt_collections";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFile {
    #[serde(default = "default_project_file_version")]
    pub version: String,
    #[serde(default = "default_mlt_collection_directory_path")]
    pub mlt_collection_directory_path: String,
    pub items: Vec<ProjectItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectItem {
    pub text: String,
}

impl Default for ProjectFile {
    fn default() -> Self {
        Self {
            version: default_project_file_version(),
            mlt_collection_directory_path: default_mlt_collection_directory_path(),
            items: Vec::new(),
        }
    }
}

impl ProjectFile {
    pub fn to_texts(&self) -> Vec<String> {
        self.items.iter().map(|item| item.text.clone()).collect()
    }

    pub fn set_texts(&mut self, texts: Vec<String>) {
        self.items = texts.into_iter().map(|text| ProjectItem { text }).collect();
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

fn default_project_file_version() -> String {
    CURRENT_PROJECT_FILE_VERSION.to_string()
}

fn default_mlt_collection_directory_path() -> String {
    DEFAULT_MLT_COLLECTION_DIRECTORY_PATH.to_string()
}
