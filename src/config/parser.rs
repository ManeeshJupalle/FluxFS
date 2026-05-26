//! TOML config loading, validation, and persistence.

use crate::config::types::FluxConfig;
use crate::errors::{FluxError, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Default config file name inside the config directory.
pub const CONFIG_FILENAME: &str = "config.toml";

/// Config subdirectory name under the platform config dir.
pub const CONFIG_DIR_NAME: &str = "fluxfs";

/// Embedded reference defaults (matches `config/default.toml` in the repo).
/// Used by tests to verify [`FluxConfig::default`] stays in sync with the
/// shipped reference TOML.
#[cfg(test)]
const EMBEDDED_DEFAULT: &str = include_str!("../../config/default.toml");

/// Returns the platform config file path (`~/.config/fluxfs/config.toml` on Unix).
pub fn config_file_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("FLUXFS_CONFIG") {
        return Ok(PathBuf::from(path));
    }

    let config_dir = dirs::config_dir().ok_or_else(|| {
        FluxError::Config(
            "Could not determine config directory. Set XDG_CONFIG_HOME or use a supported platform."
                .to_string(),
        )
    })?;
    Ok(config_dir.join(CONFIG_DIR_NAME).join(CONFIG_FILENAME))
}

/// Load config from disk; create default config with a warning if missing.
pub fn load_config() -> Result<FluxConfig> {
    let path = config_file_path()?;
    if path.exists() {
        return load_config_from_path(&path);
    }

    tracing::warn!(
        path = %path.display(),
        "Config file not found — creating default config"
    );
    let config = FluxConfig::default();
    save_config_to_path(&path, &config)?;
    Ok(config)
}

/// Load config from a specific path.
pub fn load_config_from_path(path: &Path) -> Result<FluxConfig> {
    let contents = fs::read_to_string(path).map_err(FluxError::from)?;
    parse_config_str(&contents)
}

/// Parse TOML config string with validation.
pub fn parse_config_str(contents: &str) -> Result<FluxConfig> {
    let config: FluxConfig = toml::from_str(contents).map_err(|e| {
        FluxError::Config(format!(
            "Invalid config file: {e}. Check TOML syntax and field names."
        ))
    })?;
    validate_config(&config)?;
    Ok(config)
}

/// Parse the embedded default config (must match repo `config/default.toml`).
///
/// Test-only: production code uses [`FluxConfig::default`], which is
/// hand-constructed; this helper exists to verify the reference TOML still
/// parses and matches the hand-built defaults.
#[cfg(test)]
pub fn default_from_embedded() -> Result<FluxConfig> {
    parse_config_str(EMBEDDED_DEFAULT)
}

/// Write config to the default user config path, creating parent directories.
pub fn save_default_config() -> Result<PathBuf> {
    let path = config_file_path()?;
    save_config_to_path(&path, &FluxConfig::default())
}

/// Validate and write config to the user's config file path.
pub fn save_user_config(config: &FluxConfig) -> Result<PathBuf> {
    validate_config(config)?;
    let path = config_file_path()?;
    save_config_to_path(&path, config)
}

/// Write config to a specific path.
pub fn save_config_to_path(path: &Path, config: &FluxConfig) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }
    let contents = toml::to_string_pretty(config)
        .map_err(|e| FluxError::Config(format!("Failed to serialize config: {e}")))?;
    fs::write(path, contents).map_err(FluxError::from)?;
    Ok(path.to_path_buf())
}

/// Ensure data directory exists.
pub fn ensure_data_dir(config: &FluxConfig) -> Result<PathBuf> {
    let data_dir = config.data_dir_path()?;
    fs::create_dir_all(&data_dir).map_err(FluxError::from)?;
    Ok(data_dir)
}

fn validate_config(config: &FluxConfig) -> Result<()> {
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&config.general.log_level.as_str()) {
        return Err(FluxError::Config(format!(
            "Invalid log_level '{}'. Use one of: trace, debug, info, warn, error.",
            config.general.log_level
        )));
    }

    let valid_strategies = ["report", "trash", "delete"];
    if !valid_strategies.contains(&config.duplicates.strategy.as_str()) {
        return Err(FluxError::Config(format!(
            "Invalid duplicates.strategy '{}'. Use: report, trash, or delete.",
            config.duplicates.strategy
        )));
    }

    if config.watch.is_empty() {
        return Err(FluxError::Config(
            "At least one [[watch]] entry is required.".to_string(),
        ));
    }

    for watch in &config.watch {
        crate::config::types::expand_tilde(&watch.path)?;
        for rule in &watch.rules {
            crate::config::rules::parse_rule_pattern(&rule.pattern)?;
            crate::config::types::expand_tilde(&rule.destination)?;
        }
    }

    crate::config::size::parse_size(&config.duplicates.min_size)?;
    crate::config::size::parse_size(&config.duplicates.max_hash_size)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parses_valid_config() {
        let cfg = parse_config_str(EMBEDDED_DEFAULT).expect("embedded default should parse");
        assert_eq!(cfg.general.log_level, "info");
        assert_eq!(cfg.search.max_results, 20);
    }

    #[test]
    fn embedded_default_matches_flux_config_default() {
        let embedded = default_from_embedded().expect("embedded");
        let manual = FluxConfig::default();
        assert_eq!(embedded.general, manual.general);
        assert_eq!(embedded.watch.len(), manual.watch.len());
    }

    #[test]
    fn invalid_log_level_returns_clear_error() {
        let toml = r#"
[general]
data_dir = "~/.local/share/fluxfs"
log_level = "verbose"
dry_run = false

[[watch]]
path = "~/Downloads"

[duplicates]
strategy = "trash"
min_size = "1KB"
max_hash_size = "1GB"
exclude_paths = []

[index]
exclude_patterns = []
max_depth = 20
follow_symlinks = false

[search]
max_results = 20
"#;
        let err = parse_config_str(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("log_level"), "error was: {msg}");
        assert!(msg.contains("verbose") || msg.contains("Invalid"));
    }

    #[test]
    fn invalid_toml_syntax_returns_config_error() {
        let err = parse_config_str("not valid [[toml").unwrap_err();
        assert!(err.to_string().contains("Invalid config"));
    }

    #[test]
    fn load_config_creates_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(CONFIG_FILENAME);
        assert!(!path.exists());

        std::env::set_var("FLUXFS_CONFIG", &path);
        let cfg = load_config().expect("load");
        std::env::remove_var("FLUXFS_CONFIG");

        assert!(path.exists());
        assert_eq!(cfg.duplicates.strategy, "trash");
        assert_eq!(cfg.duplicates.max_hash_size, "1GB");
        assert!(!cfg.index.follow_symlinks);
    }

    #[test]
    fn round_trip_save_and_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(CONFIG_FILENAME);
        let original = FluxConfig::default();
        save_config_to_path(&path, &original).expect("save");
        let loaded = load_config_from_path(&path).expect("load");
        assert_eq!(loaded, original);
    }

    #[test]
    fn invalid_rule_pattern_returns_error() {
        let toml = r#"
[general]
data_dir = "~/.local/share/fluxfs"
log_level = "info"
dry_run = false

[[watch]]
path = "~/Downloads"

[[watch.rules]]
pattern = "older:90x"
destination = "~/Archive"

[duplicates]
strategy = "trash"
min_size = "1KB"
max_hash_size = "1GB"
exclude_paths = []

[index]
exclude_patterns = []
max_depth = 20
follow_symlinks = false

[search]
max_results = 20
"#;
        let err = parse_config_str(toml).unwrap_err();
        assert!(err.to_string().contains("older"));
    }

    #[test]
    fn load_config_from_temp_file() {
        let mut file = NamedTempFile::new().expect("temp file");
        write!(file, "{}", EMBEDDED_DEFAULT).expect("write");
        let cfg = load_config_from_path(file.path()).expect("load");
        assert!(!cfg.watch[0].rules.is_empty());
    }
}
