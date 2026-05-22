//! File metadata extraction.

use crate::errors::{FluxError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Metadata for a single file or directory in the index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub filename: String,
    pub extension: Option<String>,
    pub size_bytes: u64,
    pub modified: DateTime<Utc>,
    pub created: Option<DateTime<Utc>>,
    pub content_hash: Option<String>,
    pub is_dir: bool,
}

impl FileEntry {
    /// Build a `FileEntry` from a path on disk.
    pub fn from_path(path: &Path) -> Result<Self> {
        let metadata = std::fs::metadata(path).map_err(|e| {
            FluxError::Scanner(format!("Cannot read metadata for {}: {e}", path.display()))
        })?;

        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase());

        let modified = metadata
            .modified()
            .map_err(FluxError::from)
            .map(DateTime::<Utc>::from)?;

        let created = metadata.created().ok().map(DateTime::<Utc>::from);

        Ok(Self {
            path: path.to_path_buf(),
            filename,
            extension,
            size_bytes: metadata.len(),
            modified,
            created,
            content_hash: None,
            is_dir: metadata.is_dir(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn from_path_extracts_file_fields() {
        let mut file = NamedTempFile::with_suffix(".txt").expect("temp file");
        write!(file, "hello fluxfs").expect("write");
        file.flush().expect("flush");

        let path = file.path();
        let entry = FileEntry::from_path(path).expect("from_path");

        assert_eq!(entry.path, path);
        assert_eq!(
            entry.filename,
            path.file_name().unwrap().to_string_lossy().as_ref()
        );
        assert_eq!(entry.extension.as_deref(), Some("txt"));
        assert!(entry.size_bytes > 0);
        assert!(!entry.is_dir);
        assert!(entry.content_hash.is_none());
    }

    #[test]
    fn from_path_handles_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let entry = FileEntry::from_path(dir.path()).expect("from_path");
        assert!(entry.is_dir);
        assert_eq!(
            entry.filename,
            dir.path().file_name().unwrap().to_string_lossy()
        );
    }
}
