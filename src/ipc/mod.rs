//! Inter-process signals shared between the daemon and tray app.

use crate::errors::{FluxError, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Flag file indicating the watcher should skip organize actions.
pub const PAUSED_FILENAME: &str = "paused";

/// Path to the pause flag inside the data directory.
pub fn paused_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PAUSED_FILENAME)
}

/// Returns true when the tray (or user) has paused the watcher.
pub fn is_paused(data_dir: &Path) -> bool {
    paused_path(data_dir).exists()
}

/// Create or remove the pause flag file.
pub fn set_paused(data_dir: &Path, paused: bool) -> Result<()> {
    fs::create_dir_all(data_dir).map_err(FluxError::from)?;
    let path = paused_path(data_dir);
    if paused {
        fs::write(&path, "1").map_err(FluxError::from)?;
    } else if path.exists() {
        fs::remove_file(path).map_err(FluxError::from)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pause_flag_round_trip() {
        let dir = tempdir().expect("tempdir");
        assert!(!is_paused(dir.path()));
        set_paused(dir.path(), true).expect("pause");
        assert!(is_paused(dir.path()));
        set_paused(dir.path(), false).expect("resume");
        assert!(!is_paused(dir.path()));
    }
}
