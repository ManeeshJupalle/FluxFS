//! Group files by hash, find duplicates, and apply resolution strategies.

use crate::config::DuplicatesConfig;
use crate::errors::{FluxError, Result};
use crate::index::store::FileIndex;
use crate::reporting::activity::{log_duplicate_found, log_duplicate_removed};
use crate::scanner::metadata::FileEntry;
use std::fs;
use std::path::{Path, PathBuf};

/// A set of files sharing identical content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub hash: String,
    pub size: u64,
    pub files: Vec<PathBuf>,
}

/// Summary of duplicate detection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DuplicateReport {
    pub groups: Vec<DuplicateGroup>,
    pub duplicate_file_count: usize,
    pub reclaimable_bytes: u64,
}

/// Result of applying a duplicate resolution strategy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolveSummary {
    pub groups_processed: usize,
    pub files_removed: usize,
    pub bytes_reclaimed: u64,
    pub dry_run: bool,
}

/// Find duplicate file groups sorted by total size (largest first).
pub fn find_duplicates(index: &FileIndex) -> Vec<DuplicateGroup> {
    let mut groups: Vec<DuplicateGroup> = index
        .hash_groups_with_duplicates()
        .into_iter()
        .filter_map(|(hash, paths)| build_group(index, hash, paths))
        .collect();

    groups.sort_by_key(|group| std::cmp::Reverse(group.size));
    groups
}

/// Build a duplicate report with aggregate statistics.
pub fn build_report(index: &FileIndex) -> DuplicateReport {
    let groups = find_duplicates(index);
    let duplicate_file_count = groups
        .iter()
        .map(|group| group.files.len().saturating_sub(1))
        .sum();
    let reclaimable_bytes = groups
        .iter()
        .map(|group| {
            group
                .size
                .saturating_mul(group.files.len().saturating_sub(1) as u64)
        })
        .sum();

    DuplicateReport {
        groups,
        duplicate_file_count,
        reclaimable_bytes,
    }
}

/// Apply the configured duplicate strategy.
pub fn resolve_duplicates(
    index: &mut FileIndex,
    config: &DuplicatesConfig,
    trash_dir: &Path,
    activity_log: &Path,
    dry_run: bool,
    confirm_delete: bool,
) -> Result<ResolveSummary> {
    let strategy = config.strategy.as_str();
    let report = build_report(index);
    let mut summary = ResolveSummary {
        groups_processed: report.groups.len(),
        dry_run,
        ..Default::default()
    };

    if dry_run || strategy == "report" {
        for group in &report.groups {
            log_duplicate_group(activity_log, index, group)?;
        }
        return Ok(summary);
    }

    if strategy == "delete" && !confirm_delete {
        return Err(FluxError::Config(
            "Delete strategy requires --confirm. Re-run with --confirm to permanently delete duplicates."
                .to_string(),
        ));
    }

    if strategy == "trash" && !dry_run {
        fs::create_dir_all(trash_dir).map_err(FluxError::from)?;
    }

    for group in &report.groups {
        let keep = choose_original(index, &group.files)?;
        for path in &group.files {
            if path == &keep {
                continue;
            }

            let size_bytes = index
                .get(path)
                .map(|entry| entry.size_bytes)
                .ok_or_else(|| {
                    FluxError::Index(format!(
                        "Indexed file missing during dedup: {}",
                        path.display()
                    ))
                })?;

            log_duplicate_found(activity_log, &keep, path, size_bytes)?;

            match strategy {
                "trash" => {
                    let destination = unique_trash_path(trash_dir, path);
                    fs::rename(path, &destination).map_err(|e| {
                        FluxError::Io(std::io::Error::new(
                            e.kind(),
                            format!("Failed to move duplicate {} to trash: {e}", path.display()),
                        ))
                    })?;
                    log_duplicate_removed(activity_log, path, size_bytes)?;
                    index.remove(path);
                    summary.files_removed += 1;
                    summary.bytes_reclaimed += size_bytes;
                }
                "delete" => {
                    fs::remove_file(path).map_err(|e| {
                        FluxError::Io(std::io::Error::new(
                            e.kind(),
                            format!("Failed to delete duplicate {}: {e}", path.display()),
                        ))
                    })?;
                    log_duplicate_removed(activity_log, path, size_bytes)?;
                    index.remove(path);
                    summary.files_removed += 1;
                    summary.bytes_reclaimed += size_bytes;
                }
                _ => {}
            }
        }
    }

    index.rebuild_hash_groups();
    Ok(summary)
}

fn build_group(index: &FileIndex, hash: String, paths: Vec<PathBuf>) -> Option<DuplicateGroup> {
    if paths.len() < 2 {
        return None;
    }

    let size = paths
        .first()
        .and_then(|path| index.get(path))
        .map(|entry| entry.size_bytes)
        .unwrap_or(0);

    Some(DuplicateGroup {
        hash,
        size,
        files: paths,
    })
}

fn choose_original(index: &FileIndex, files: &[PathBuf]) -> Result<PathBuf> {
    let mut sorted: Vec<&FileEntry> = files.iter().filter_map(|path| index.get(path)).collect();

    if sorted.is_empty() {
        return Err(FluxError::Index(
            "Duplicate group has no indexed entries.".to_string(),
        ));
    }

    sorted.sort_by_key(|entry| entry.modified);
    Ok(sorted[0].path.clone())
}

fn log_duplicate_group(
    activity_log: &Path,
    index: &FileIndex,
    group: &DuplicateGroup,
) -> Result<()> {
    let keep = choose_original(index, &group.files)?;
    for path in &group.files {
        if path == &keep {
            continue;
        }
        let size = index.get(path).map(|e| e.size_bytes).unwrap_or(group.size);
        log_duplicate_found(activity_log, &keep, path, size)?;
    }
    Ok(())
}

fn unique_trash_path(trash_dir: &Path, source: &Path) -> PathBuf {
    let filename = source
        .file_name()
        .map(|name| name.to_owned())
        .unwrap_or_else(|| source.as_os_str().to_os_string());

    let mut candidate = trash_dir.join(filename);
    let mut counter = 1u32;

    while candidate.exists() {
        let stem = source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        let ext = source
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        candidate = trash_dir.join(format!("{stem}_{counter}{ext}"));
        counter += 1;
    }

    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::store::FileIndex;
    use crate::scanner::metadata::FileEntry;
    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    fn entry(path: PathBuf, hash: &str, modified_offset_secs: i64) -> FileEntry {
        FileEntry {
            path: path.clone(),
            filename: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            extension: Some("txt".to_string()),
            size_bytes: 10,
            modified: Utc::now() - Duration::seconds(modified_offset_secs),
            created: None,
            content_hash: Some(hash.to_string()),
            is_dir: false,
        }
    }

    #[test]
    fn find_duplicates_returns_sorted_groups() {
        let mut index = FileIndex::new();
        index.insert(entry(PathBuf::from("a.txt"), "hash-a", 10));
        index.insert(entry(PathBuf::from("b.txt"), "hash-a", 5));
        index.insert(entry(PathBuf::from("c.txt"), "hash-b", 1));
        index.insert(entry(PathBuf::from("d.txt"), "hash-b", 2));

        let groups = find_duplicates(&index);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn trash_strategy_moves_duplicate_and_keeps_oldest() {
        let dir = tempdir().expect("tempdir");
        let trash_dir = dir.path().join("trash");
        let log_path = dir.path().join("activity.jsonl");

        let original = dir.path().join("original.txt");
        let duplicate = dir.path().join("duplicate.txt");
        std::fs::write(&original, b"same").expect("write");
        std::fs::write(&duplicate, b"same").expect("write");

        let mut index = FileIndex::new();
        index.insert(entry(original.clone(), "dup-hash", 100));
        index.insert(entry(duplicate.clone(), "dup-hash", 10));

        let cfg = DuplicatesConfig {
            strategy: "trash".to_string(),
            min_size: "1B".to_string(),
            exclude_paths: vec![],
        };

        let summary = resolve_duplicates(&mut index, &cfg, &trash_dir, &log_path, false, false)
            .expect("resolve");

        assert_eq!(summary.files_removed, 1);
        assert!(original.exists());
        assert!(!duplicate.exists());
        assert!(trash_dir.read_dir().unwrap().next().is_some());
        assert!(index.get(&duplicate).is_none());
    }

    #[test]
    fn dry_run_does_not_move_files() {
        let dir = tempdir().expect("tempdir");
        let trash_dir = dir.path().join("trash");
        let log_path = dir.path().join("activity.jsonl");

        let original = dir.path().join("keep.txt");
        let duplicate = dir.path().join("dup.txt");
        std::fs::write(&original, b"x").expect("write");
        std::fs::write(&duplicate, b"x").expect("write");

        let mut index = FileIndex::new();
        index.insert(entry(original.clone(), "h1", 20));
        index.insert(entry(duplicate.clone(), "h1", 5));

        let cfg = DuplicatesConfig {
            strategy: "trash".to_string(),
            min_size: "1B".to_string(),
            exclude_paths: vec![],
        };

        resolve_duplicates(&mut index, &cfg, &trash_dir, &log_path, true, false).expect("resolve");

        assert!(duplicate.exists());
        assert!(index.get(&duplicate).is_some());
    }
}
