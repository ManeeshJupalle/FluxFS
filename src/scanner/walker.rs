//! Directory traversal with walkdir.

use crate::config::IndexConfig;
use crate::errors::Result;
use crate::scanner::metadata::FileEntry;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

/// Summary of a directory scan operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSummary {
    pub file_count: usize,
    pub total_size_bytes: u64,
    pub directories_scanned: usize,
    pub duration_ms: u64,
}

/// Scan a single directory tree and return file entries (directories are walked but not indexed).
pub fn scan_directory(root: &Path, index_cfg: &IndexConfig) -> Result<Vec<FileEntry>> {
    let paths = collect_file_paths(root, index_cfg)?;
    let entries: Vec<FileEntry> = paths
        .par_iter()
        .filter_map(|path| match FileEntry::from_path(path) {
            Ok(entry) => Some(entry),
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "Skipping file (permission denied or unreadable)"
                );
                None
            }
        })
        .collect();
    Ok(entries)
}

/// Scan multiple roots and return combined entries plus summary statistics.
pub fn scan_directories(
    roots: &[PathBuf],
    index_cfg: &IndexConfig,
) -> Result<(Vec<FileEntry>, ScanSummary)> {
    let started = Instant::now();
    let mut all_entries = Vec::new();
    let mut directories_scanned = 0usize;

    for root in roots {
        if !root.exists() {
            tracing::warn!(
                path = %root.display(),
                "Watch directory does not exist, skipping"
            );
            continue;
        }
        if !root.is_dir() {
            tracing::warn!(
                path = %root.display(),
                "Watch path is not a directory, skipping"
            );
            continue;
        }

        directories_scanned += 1;
        let mut entries = scan_directory(root, index_cfg)?;
        all_entries.append(&mut entries);
    }

    let duration_ms = started.elapsed().as_millis() as u64;
    let file_count = all_entries.len();
    let total_size_bytes = all_entries.iter().map(|e| e.size_bytes).sum();

    Ok((
        all_entries,
        ScanSummary {
            file_count,
            total_size_bytes,
            directories_scanned,
            duration_ms,
        },
    ))
}

fn collect_file_paths(root: &Path, index_cfg: &IndexConfig) -> Result<Vec<PathBuf>> {
    let max_depth = index_cfg.max_depth;
    let exclude = &index_cfg.exclude_patterns;
    let follow_symlinks = index_cfg.follow_symlinks;

    let paths: Vec<PathBuf> = WalkDir::new(root)
        .max_depth(max_depth as usize)
        .follow_links(follow_symlinks)
        .into_iter()
        .filter_entry(|entry| !should_skip_entry(entry.path(), entry.file_type().is_dir(), exclude))
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(err) => {
                tracing::warn!(error = %err, "Permission denied during scan, skipping entry");
                None
            }
        })
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| follow_symlinks || !entry.file_type().is_symlink())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy();
            !matches_exclude_name(&name, exclude)
        })
        .map(|entry| entry.into_path())
        .collect();

    Ok(paths)
}

fn should_skip_entry(path: &Path, is_dir: bool, exclude_patterns: &[String]) -> bool {
    if !is_dir {
        return false;
    }
    path.file_name()
        .map(|name| matches_exclude_name(&name.to_string_lossy(), exclude_patterns))
        .unwrap_or(false)
}

fn matches_exclude_name(name: &str, exclude_patterns: &[String]) -> bool {
    exclude_patterns
        .iter()
        .any(|pattern| name == pattern || name.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn index_cfg(max_depth: u32, follow_symlinks: bool) -> IndexConfig {
        IndexConfig {
            exclude_patterns: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                ".venv".to_string(),
            ],
            max_depth,
            follow_symlinks,
        }
    }

    #[test]
    fn scan_empty_directory_returns_no_entries() {
        let dir = tempdir().expect("tempdir");
        let entries = scan_directory(dir.path(), &index_cfg(20, false)).expect("scan");
        assert!(entries.is_empty());
    }

    #[test]
    fn scan_directory_collects_files() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("a.txt"), b"aaa").expect("write");
        fs::write(dir.path().join("b.pdf"), b"bbb").expect("write");

        let entries = scan_directory(dir.path(), &index_cfg(20, false)).expect("scan");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| !e.is_dir));
    }

    #[test]
    fn scan_skips_excluded_directories() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("node_modules")).expect("mkdir");
        fs::write(dir.path().join("node_modules/hidden.js"), b"x").expect("write");
        fs::write(dir.path().join("visible.txt"), b"y").expect("write");

        let entries = scan_directory(dir.path(), &index_cfg(20, false)).expect("scan");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "visible.txt");
    }

    #[test]
    fn scan_respects_max_depth() {
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("level1").join("level2");
        fs::create_dir_all(&nested).expect("mkdir");
        fs::write(dir.path().join("root.txt"), b"r").expect("write");
        fs::write(dir.path().join("level1/mid.txt"), b"m").expect("write");
        fs::write(nested.join("deep.txt"), b"d").expect("write");

        let entries = scan_directory(dir.path(), &index_cfg(1, false)).expect("scan");
        let names: Vec<_> = entries.iter().map(|e| e.filename.as_str()).collect();
        assert!(names.contains(&"root.txt"));
        assert!(!names.contains(&"deep.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn scan_skips_symlinks_by_default() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("real.txt");
        let link = dir.path().join("link.txt");
        fs::write(&target, b"data").expect("write");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let entries = scan_directory(dir.path(), &index_cfg(20, false)).expect("scan");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "real.txt");
    }

    #[cfg(unix)]
    #[test]
    fn scan_follows_symlinks_when_enabled() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("real.txt");
        let link = dir.path().join("link.txt");
        fs::write(&target, b"data").expect("write");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        let entries = scan_directory(dir.path(), &index_cfg(20, true)).expect("scan");
        assert_eq!(entries.len(), 2);
    }
}
