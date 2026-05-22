//! Application error types.

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
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Convenience result type for FluxFS operations.
pub type Result<T> = std::result::Result<T, FluxError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_error_formats_user_message() {
        let err = FluxError::Config("Invalid log_level 'verbose'.".to_string());
        assert!(err.to_string().contains("Config error"));
        assert!(err.to_string().contains("verbose"));
    }

    #[test]
    fn io_error_converts_from_std_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing file");
        let err: FluxError = io_err.into();
        assert!(matches!(err, FluxError::Io(_)));
    }
}
