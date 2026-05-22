//! SHA-256 content hashing with parallel batch processing.

use crate::config::DuplicatesConfig;
use crate::errors::{FluxError, Result};
use crate::index::store::FileIndex;
use crate::scanner::metadata::FileEntry;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

const READ_BUFFER_SIZE: usize = 8192;
const PROGRESS_INTERVAL: usize = 500;

/// Statistics from a batch hashing run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HashStats {
    pub hashed: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Compute SHA-256 hex digest for a file on disk.
///
/// Test-only helper. Production code uses [`hash_file_with_mtime`] directly
/// so it can record the modified time observed at hash time alongside the
/// digest (required for stale-hash detection).
#[cfg(test)]
pub fn hash_file(path: &Path) -> Result<String> {
    hash_file_with_mtime(path).map(|(digest, _)| digest)
}

/// Like [`hash_file`], but also returns the file's modified time as observed
/// after the read completed. The stamp lets the index detect later mtime
/// changes and invalidate the cached hash.
fn hash_file_with_mtime(path: &Path) -> Result<(String, DateTime<Utc>)> {
    let mut file = File::open(path).map_err(|e| {
        FluxError::Scanner(format!("Cannot open {} for hashing: {e}", path.display()))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; READ_BUFFER_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer).map_err(|e| {
            FluxError::Scanner(format!("Cannot read {} while hashing: {e}", path.display()))
        })?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let modified: DateTime<Utc> = file
        .metadata()
        .map_err(|e| {
            FluxError::Scanner(format!(
                "Cannot read mtime for {} after hashing: {e}",
                path.display()
            ))
        })?
        .modified()
        .map_err(|e| {
            FluxError::Scanner(format!(
                "Filesystem does not expose mtime for {}: {e}",
                path.display()
            ))
        })?
        .into();

    Ok((format!("{:x}", hasher.finalize()), modified))
}

/// Hash all unhashed entries in the index in parallel.
pub fn hash_all(index: &mut FileIndex, duplicates: &DuplicatesConfig) -> Result<HashStats> {
    let min_size = parse_min_size(&duplicates.min_size)?;
    let max_size = parse_min_size(&duplicates.max_hash_size)?;
    let exclude_paths = &duplicates.exclude_paths;

    let unhashed = index.entries_needing_hash();
    let candidates: Vec<(PathBuf, FileEntry)> = unhashed
        .iter()
        .filter(|(_, entry)| {
            let skip = should_skip_entry(entry, min_size, max_size, exclude_paths);
            if skip && entry.size_bytes > max_size {
                tracing::warn!(
                    path = %entry.path.display(),
                    size = entry.size_bytes,
                    max = max_size,
                    "Skipping hash for large file"
                );
            }
            !skip
        })
        .cloned()
        .collect();

    let skipped = unhashed.len().saturating_sub(candidates.len());
    let total = candidates.len();

    if total == 0 {
        return Ok(HashStats {
            hashed: 0,
            skipped,
            failed: 0,
        });
    }

    info!(files = total, "Hashing file contents");

    let completed = AtomicUsize::new(0);
    let mut updates = Vec::new();
    let mut failed = 0usize;

    type HashResult = Option<(String, DateTime<Utc>)>;
    let results: Vec<(PathBuf, HashResult)> = candidates
        .par_iter()
        .map(|(path, _)| {
            let hash = hash_file_with_mtime(path).ok();
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if done == total || done.is_multiple_of(PROGRESS_INTERVAL) {
                let pct = (done as f64 / total as f64) * 100.0;
                info!(
                    hashed = done,
                    total,
                    progress_pct = format!("{pct:.0}"),
                    "Hashing progress"
                );
            }
            (path.clone(), hash)
        })
        .collect();

    for (path, hash) in results {
        match hash {
            Some((digest, mtime)) => updates.push((path, digest, mtime)),
            None => failed += 1,
        }
    }

    index.apply_content_hashes(updates);

    Ok(HashStats {
        hashed: total - failed,
        skipped,
        failed,
    })
}

fn should_skip_entry(
    entry: &FileEntry,
    min_size: u64,
    max_size: u64,
    exclude_paths: &[String],
) -> bool {
    if entry.size_bytes < min_size || entry.size_bytes > max_size {
        return true;
    }
    path_matches_exclude(&entry.path, exclude_paths)
}

fn path_matches_exclude(path: &Path, exclude_paths: &[String]) -> bool {
    path.components().any(|component| {
        let part = component.as_os_str().to_string_lossy();
        exclude_paths
            .iter()
            .any(|pattern| part == pattern.as_str() || part.contains(pattern))
    })
}

/// Parse human-readable size strings such as `1KB`, `10MB`, `2GB`.
pub fn parse_min_size(value: &str) -> Result<u64> {
    crate::config::size::parse_size(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::store::FileIndex;
    use crate::scanner::metadata::FileEntry;
    use chrono::Utc;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn entry_at(path: PathBuf, size: u64) -> FileEntry {
        FileEntry {
            filename: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            extension: path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase()),
            size_bytes: size,
            modified: Utc::now(),
            created: None,
            content_hash: None,
            hash_modified: None,
            is_dir: false,
            path,
        }
    }

    #[test]
    fn hash_file_is_deterministic() {
        let mut file = NamedTempFile::new().expect("temp file");
        write!(file, "fluxfs-test-payload").expect("write");
        file.flush().expect("flush");

        let first = hash_file(file.path()).expect("hash");
        let second = hash_file(file.path()).expect("hash");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn hash_known_content_matches_sha256() {
        let mut file = NamedTempFile::new().expect("temp file");
        write!(file, "abc").expect("write");
        file.flush().expect("flush");

        let digest = hash_file(file.path()).expect("hash");
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parse_min_size_supports_kb_and_mb() {
        assert_eq!(parse_min_size("1KB").expect("kb"), 1024);
        assert_eq!(parse_min_size("2MB").expect("mb"), 2 * 1024 * 1024);
    }

    #[test]
    fn hash_all_parallel_matches_sequential() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path_a = dir.path().join("a.txt");
        let path_b = dir.path().join("b.txt");
        std::fs::write(&path_a, b"same-content").expect("write");
        std::fs::write(&path_b, b"same-content").expect("write");

        let mut index = FileIndex::new();
        index.insert(entry_at(path_a.clone(), 12));
        index.insert(entry_at(path_b.clone(), 12));

        let cfg = DuplicatesConfig {
            strategy: "report".to_string(),
            min_size: "1B".to_string(),
            max_hash_size: "1GB".to_string(),
            exclude_paths: vec![],
        };

        let stats = hash_all(&mut index, &cfg).expect("hash all");
        assert_eq!(stats.hashed, 2);

        let hash_a = index.get(&path_a).unwrap().content_hash.clone().unwrap();
        let hash_b = index.get(&path_b).unwrap().content_hash.clone().unwrap();
        assert_eq!(hash_a, hash_b);
    }

    #[test]
    fn hash_all_skips_files_above_max_hash_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("huge.bin");
        std::fs::write(&path, vec![0u8; 64]).expect("write");

        let mut index = FileIndex::new();
        index.insert(FileEntry {
            size_bytes: 2 * 1024 * 1024,
            ..entry_at(path.clone(), 2 * 1024 * 1024)
        });

        let cfg = DuplicatesConfig {
            strategy: "report".to_string(),
            min_size: "1B".to_string(),
            max_hash_size: "1KB".to_string(),
            exclude_paths: vec![],
        };

        let stats = hash_all(&mut index, &cfg).expect("hash all");
        assert_eq!(stats.hashed, 0);
        assert_eq!(stats.skipped, 1);
        assert!(index.get(&path).unwrap().content_hash.is_none());
    }

    #[test]
    fn hash_all_rehashes_when_mtime_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("file.txt");
        std::fs::write(&path, b"first").expect("write v1");

        let mut index = FileIndex::new();
        let original_entry = FileEntry::from_path(&path).expect("entry");
        index.insert(original_entry);

        let cfg = DuplicatesConfig {
            strategy: "report".to_string(),
            min_size: "1B".to_string(),
            max_hash_size: "1GB".to_string(),
            exclude_paths: vec![],
        };

        let stats = hash_all(&mut index, &cfg).expect("hash 1");
        assert_eq!(stats.hashed, 1);
        let first_hash = index.get(&path).unwrap().content_hash.clone();
        let first_stamp = index.get(&path).unwrap().hash_modified;
        assert!(first_hash.is_some());
        assert!(first_stamp.is_some());

        // Rewrite with new content and force a different mtime, then re-stat
        // the file to update the index entry's modified field — simulating a
        // scan that observed the change.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&path, b"second-different-size").expect("write v2");

        // Calling hash_all WITHOUT updating the index entry: the staleness
        // filter must restat the file and detect the divergence on its own.
        let stats2 = hash_all(&mut index, &cfg).expect("hash 2");
        assert_eq!(
            stats2.hashed, 1,
            "stale entry should be re-hashed (current mtime != stored stamp)"
        );

        let second_hash = index.get(&path).unwrap().content_hash.clone();
        assert!(second_hash.is_some());
        assert_ne!(
            first_hash, second_hash,
            "hash should change when file content changes"
        );
    }

    #[test]
    fn hash_all_skips_files_below_min_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tiny.txt");
        std::fs::write(&path, b"x").expect("write");

        let mut index = FileIndex::new();
        index.insert(entry_at(path.clone(), 1));

        let cfg = DuplicatesConfig {
            strategy: "report".to_string(),
            min_size: "1KB".to_string(),
            max_hash_size: "1GB".to_string(),
            exclude_paths: vec![],
        };

        let stats = hash_all(&mut index, &cfg).expect("hash all");
        assert_eq!(stats.hashed, 0);
        assert_eq!(stats.skipped, 1);
        assert!(index.get(&path).unwrap().content_hash.is_none());
    }
}
