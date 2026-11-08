//! The extras' settings, in `extras.json` beside upstream's settings so a
//! merge never touches them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Download every attachment as it arrives (REQ-EXTRAS-008).
    pub save_all_media: bool,
    /// Read message text, in memory, for the Words tab (REQ-EXTRAS-004).
    pub content_analysis: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            save_all_media: true,
            content_analysis: false,
        }
    }
}

impl Config {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("extras.json")
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Hands the file to `files`, which writes it off the interface thread.
    pub fn save_to(&self, files: &fastframe_file::Writer, path: &Path) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            files.write(path, text, fastframe_file::Access::Default);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_analysis_is_off_by_default() {
        let config = Config::default();
        assert!(!config.content_analysis);
        assert!(config.save_all_media);
        let partial: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(partial, config);
    }
}
