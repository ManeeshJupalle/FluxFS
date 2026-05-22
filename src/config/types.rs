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
    /// Skip hashing files larger than this (default 1GB).
    #[serde(default = "default_max_hash_size")]
    pub max_hash_size: String,
    pub exclude_paths: Vec<String>,
}

fn default_max_hash_size() -> String {
    "1GB".to_string()
}

/// Indexing and scan settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexConfig {
    pub exclude_patterns: Vec<String>,
    pub max_depth: u32,
    /// Follow symbolic links during scans (default: skip).
    #[serde(default)]
    pub follow_symlinks: bool,
}

/// Search command settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchConfig {
    pub max_results: usize,
}

impl Default for FluxConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                data_dir: "~/.local/share/fluxfs".to_string(),
                log_level: "info".to_string(),
                dry_run: false,
            },
            watch: vec![WatchConfig {
                path: "~/Downloads".to_string(),
                rules: vec![
                    WatchRule {
                        pattern: "*.pdf".to_string(),
                        destination: "~/Documents/PDFs/".to_string(),
                    },
                    WatchRule {
                        pattern: "*.png,*.jpg,*.jpeg,*.gif,*.webp".to_string(),
                        destination: "~/Pictures/Organized/".to_string(),
                    },
                    WatchRule {
                        pattern: "*.dmg,*.exe,*.msi,*.pkg".to_string(),
                        destination: "~/Installers/".to_string(),
                    },
                    WatchRule {
                        pattern: "*.zip,*.tar.gz,*.rar,*.7z".to_string(),
                        destination: "~/Archives/".to_string(),
                    },
                ],
            }],
            duplicates: DuplicatesConfig {
                strategy: "trash".to_string(),
                min_size: "1KB".to_string(),
                max_hash_size: "1GB".to_string(),
                exclude_paths: vec![
                    ".git".to_string(),
                    "node_modules".to_string(),
                    ".venv".to_string(),
                ],
            },
            index: IndexConfig {
                exclude_patterns: vec![
                    ".git".to_string(),
                    "node_modules".to_string(),
                    ".venv".to_string(),
                    "__pycache__".to_string(),
                    ".DS_Store".to_string(),
                ],
                max_depth: 20,
                follow_symlinks: false,
            },
            search: SearchConfig { max_results: 20 },
        }
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
