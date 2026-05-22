//! Application error types.

use std::io;
use thiserror::Error;

/// FluxFS library and CLI errors.
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum FluxError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Scanner error: {0}")]
    Scanner(String),

    #[error("Watcher error: {0}")]
    Watcher(String),

    #[error("Rule error: {0}")]
    Rule(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Convenience result type for FluxFS operations.
pub type Result<T> = std::result::Result<T, FluxError>;

impl FluxError {
    /// Optional actionable hint for CLI users.
    pub fn user_hint(&self) -> Option<String> {
        let config_hint = format!(
            "Update your config at {} or set FLUXFS_CONFIG to a custom path.",
            config_hint_path()
        );

        match self {
            FluxError::Config(_) => Some(config_hint),
            FluxError::Index(msg) if msg.contains("empty") => {
                Some("Run `flux init` to scan watch directories and build the index.".into())
            }
            FluxError::Watcher(msg)
                if msg.contains("does not exist")
                    || msg.contains("Cannot watch")
                    || msg.contains("not a directory") =>
            {
                Some(format!(
                    "Create the directory or fix the watch path. {config_hint}"
                ))
            }
            FluxError::Watcher(msg) if msg.contains("already running") => {
                Some("Stop the existing daemon with `flux stop` before starting again.".into())
            }
            FluxError::Watcher(msg) if msg.contains("not running") => {
                Some("Start the daemon with `flux start --foreground`.".into())
            }
            FluxError::Rule(msg) if msg.contains("full") || msg.contains("space") => {
                Some("Free disk space on the destination volume, then retry the operation.".into())
            }
            FluxError::Rule(_) => Some(
                "Check destination paths in your config and ensure FluxFS can write there.".into(),
            ),
            FluxError::Io(err) if err.kind() == io::ErrorKind::StorageFull => {
                Some("Filesystem is full. Free space before moving or copying files.".into())
            }
            FluxError::Io(err) if err.kind() == io::ErrorKind::PermissionDenied => {
                Some("Check file permissions or run with appropriate access.".into())
            }
            _ => None,
        }
    }
}

/// Extract a user hint from an anyhow error chain (FluxError or wrapped IO).
pub fn hint_for_anyhow(err: &anyhow::Error) -> Option<String> {
    if let Some(flux) = err.downcast_ref::<FluxError>() {
        if let Some(hint) = flux.user_hint() {
            return Some(hint);
        }
    }

    let message = err.to_string();
    if message.contains("Filesystem is full") || message.contains("full") {
        return Some("Free disk space on the destination volume, then retry.".into());
    }
    if message.contains("Cannot watch") || message.contains("does not exist") {
        return Some(format!(
            "Create the directory or update your config at {}.",
            config_hint_path()
        ));
    }
    if message.contains("Index is empty") {
        return Some("Run `flux init` first to scan and build the index.".into());
    }
    if message.contains("Config error") {
        return Some(format!(
            "Check TOML syntax and fields in {}.",
            config_hint_path()
        ));
    }
    None
}

fn config_hint_path() -> String {
    std::env::var("FLUXFS_CONFIG").unwrap_or_else(|_| {
        dirs::config_dir()
            .map(|dir| dir.join("fluxfs").join("config.toml").display().to_string())
            .unwrap_or_else(|| "~/.config/fluxfs/config.toml".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_error_formats_user_message() {
        let err = FluxError::Config("Invalid log_level 'verbose'.".to_string());
        assert!(err.to_string().contains("Config error"));
        assert!(err.user_hint().is_some());
    }

    #[test]
    fn io_error_converts_from_std_io() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing file");
        let err: FluxError = io_err.into();
        assert!(matches!(err, FluxError::Io(_)));
    }

    #[test]
    fn watcher_missing_dir_includes_hint() {
        let err = FluxError::Watcher("Cannot watch ~/Downloads — directory does not exist.".into());
        let hint = err.user_hint().expect("hint");
        assert!(hint.contains("config"));
    }
}
