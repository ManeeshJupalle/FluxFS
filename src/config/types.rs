//! Config structs deserialized from TOML.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Root FluxFS configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluxConfig {
    pub general: GeneralConfig,
    pub watch: Vec<WatchConfig>,
    pub duplicates: DuplicatesConfig,
    pub index: IndexConfig,
    pub search: SearchConfig,
}

/// General application settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub data_dir: String,
    pub log_level: String,
    pub dry_run: bool,
}

/// A watched directory with optional organization rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchConfig {
    pub path: String,
    #[serde(default)]
    pub rules: Vec<WatchRule>,
}

/// A single file organization rule for a watch path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchRule {
    pub pattern: String,
    pub destination: String,
}

/// Duplicate detection and handling settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicatesConfig {
    pub strategy: String,
    pub min_size: String,
    pub exclude_paths: Vec<String>,
}

/// Indexing and scan settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexConfig {
    pub exclude_patterns: Vec<String>,
    pub max_depth: u32,
}

/// Search command settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchConfig {
    pub max_results: usize,
}

impl Default for FluxConfig {
    fn default() -> Self {
        crate::config::parser::default_from_embedded()
            .expect("embedded config/default.toml must be valid")
    }
}

impl FluxConfig {
    /// Resolved data directory (tilde expanded).
    pub fn data_dir_path(&self) -> crate::errors::Result<PathBuf> {
        expand_tilde(&self.general.data_dir)
    }

    /// Watch directory paths (tilde expanded).
    pub fn watch_paths(&self) -> crate::errors::Result<Vec<PathBuf>> {
        self.watch.iter().map(|w| expand_tilde(&w.path)).collect()
    }
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(path: &str) -> crate::errors::Result<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().ok_or_else(|| {
            crate::errors::FluxError::Config(
                "Could not determine home directory for path expansion.".to_string(),
            )
        })?;
        Ok(home.join(rest))
    } else if path == "~" {
        dirs::home_dir().ok_or_else(|| {
            crate::errors::FluxError::Config("Could not determine home directory.".to_string())
        })
    } else {
        Ok(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_downloads_watch() {
        let cfg = FluxConfig::default();
        assert_eq!(cfg.watch.len(), 1);
        assert_eq!(cfg.watch[0].path, "~/Downloads");
        assert_eq!(cfg.watch[0].rules.len(), 4);
    }

    #[test]
    fn expand_tilde_joins_home() {
        let home = dirs::home_dir().expect("home dir");
        let expanded = expand_tilde("~/Downloads").expect("expand");
        assert_eq!(expanded, home.join("Downloads"));
    }
}
