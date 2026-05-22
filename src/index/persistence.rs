//! Serialize/deserialize index to disk.

use crate::config::FluxConfig;
use crate::errors::{FluxError, Result};
use crate::index::store::FileIndex;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;

/// Index filename inside the data directory.
pub const INDEX_FILENAME: &str = "index.bin";

/// Path to the on-disk index file for a configuration.
pub fn index_file_path(config: &FluxConfig) -> Result<PathBuf> {
    Ok(config.data_dir_path()?.join(INDEX_FILENAME))
}

/// Save the index to disk using bincode.
pub fn save(index: &FileIndex, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }
    let bytes = bincode::serialize(index)
        .map_err(|e| FluxError::Serialization(format!("Failed to serialize index: {e}")))?;
    fs::write(path, bytes).map_err(FluxError::from)?;
    Ok(())
}

/// Load the index from disk. Missing or corrupt files return an empty index.
pub fn load(path: &Path) -> Result<FileIndex> {
    if !path.exists() {
        return Ok(FileIndex::new());
    }

    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(FileIndex::new()),
        Err(err) => return Err(FluxError::from(err)),
    };

    if bytes.is_empty() {
        warn!(path = %path.display(), "Index file is empty, starting fresh");
        return Ok(FileIndex::new());
    }

    match bincode::deserialize::<FileIndex>(&bytes) {
        Ok(index) => Ok(index),
        Err(err) => {
            warn!(
                path = %path.display(),
                error = %err,
                "Index file is corrupt or incompatible, starting fresh"
            );
            Ok(FileIndex::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::store::FileIndex;
    use crate::scanner::metadata::FileEntry;
    use chrono::Utc;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn sample_entry() -> FileEntry {
        FileEntry {
            path: PathBuf::from("/tmp/example.txt"),
            filename: "example.txt".to_string(),
            extension: Some("txt".to_string()),
            size_bytes: 42,
            modified: Utc::now(),
            created: None,
            content_hash: None,
            is_dir: false,
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("index.bin");

        let mut index = FileIndex::new();
        index.insert(sample_entry());
        save(&index, &path).expect("save");

        let loaded = load(&path).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.stats().total_files, 1);
    }

    #[test]
    fn load_missing_file_returns_empty_index() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("missing.bin");
        let index = load(&path).expect("load");
        assert!(index.is_empty());
    }

    #[test]
    fn load_corrupt_file_returns_empty_index() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("index.bin");
        std::fs::write(&path, b"not-valid-bincode").expect("write");

        let index = load(&path).expect("load");
        assert!(index.is_empty());
    }
}
