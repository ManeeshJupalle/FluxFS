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
///
/// The write is atomic: bytes are first written to a sibling `<name>.tmp`
/// file, then `fs::rename` swaps it into place. A crash or power loss in the
/// middle of the write leaves the original `index.bin` intact (or, if there
/// was none, leaves only the `.tmp` file behind, which the next save
/// overwrites). On Rust 1.85+, `fs::rename` replaces an existing destination
/// on both Unix and Windows; older toolchains fall through to a
/// `remove_file` + `rename` retry.
pub fn save(index: &FileIndex, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }
    let bytes = bincode::serialize(index)
        .map_err(|e| FluxError::Serialization(format!("Failed to serialize index: {e}")))?;

    let tmp_path = tmp_sibling(path);
    fs::write(&tmp_path, &bytes).map_err(|e| {
        FluxError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to write temp index {}: {e}", tmp_path.display()),
        ))
    })?;

    if let Err(err) = fs::rename(&tmp_path, path) {
        // Older Windows toolchains can reject rename-over-existing.
        if path.exists() && fs::remove_file(path).is_ok() {
            fs::rename(&tmp_path, path).map_err(|e| {
                FluxError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to swap index {} into place: {e}", path.display()),
                ))
            })?;
        } else {
            // Best-effort cleanup of the leftover temp file.
            let _ = fs::remove_file(&tmp_path);
            return Err(FluxError::Io(std::io::Error::new(
                err.kind(),
                format!("Failed to swap index {} into place: {err}", path.display()),
            )));
        }
    }

    Ok(())
}

fn tmp_sibling(path: &Path) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let mut tmp = path.to_path_buf();
    tmp.set_file_name(name);
    tmp
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
            hash_modified: None,
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
