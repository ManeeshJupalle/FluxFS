//! File move/copy operations with safety checks.

use crate::errors::{FluxError, Result};
use crate::reporting::activity::log_file_moved;
use crate::rules::engine::{Rule, RuleAction};
use crate::scanner::metadata::FileEntry;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Outcome of organizing a single file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizeResult {
    DryRun { from: PathBuf, to: PathBuf },
    Moved { from: PathBuf, to: PathBuf },
    Copied { from: PathBuf, to: PathBuf },
    Skipped { path: PathBuf, reason: String },
}

/// Move or copy a file according to a rule, with safety checks.
pub fn organize_file(
    entry: &FileEntry,
    rule: &Rule,
    dry_run: bool,
    activity_log: &Path,
) -> Result<OrganizeResult> {
    // `resolve_destination` only inspects whether the destination directory
    // currently contains a file with the same name; it must not create the
    // directory itself, or dry-run would leave behind empty folders.
    let dest_path = resolve_destination(&rule.destination, &entry.filename);

    if paths_equivalent(&entry.path, &dest_path) {
        return Ok(OrganizeResult::Skipped {
            path: entry.path.clone(),
            reason: "File is already at the destination.".to_string(),
        });
    }

    if dry_run {
        return Ok(OrganizeResult::DryRun {
            from: entry.path.clone(),
            to: dest_path,
        });
    }

    // Real run only: create the destination directory now.
    fs::create_dir_all(&rule.destination).map_err(|e| {
        FluxError::Rule(format!(
            "Cannot create destination directory {}: {e}",
            rule.destination.display()
        ))
    })?;

    match rule.action {
        RuleAction::Move => {
            move_file(&entry.path, &dest_path).map_err(map_filesystem_error)?;
            log_file_moved(activity_log, &entry.path, &dest_path, &rule.label)?;
            Ok(OrganizeResult::Moved {
                from: entry.path.clone(),
                to: dest_path,
            })
        }
        RuleAction::Copy => {
            fs::copy(&entry.path, &dest_path).map_err(map_filesystem_error)?;
            log_file_moved(activity_log, &entry.path, &dest_path, &rule.label)?;
            Ok(OrganizeResult::Copied {
                from: entry.path.clone(),
                to: dest_path,
            })
        }
    }
}

/// Move a file with a cross-device fallback: if `fs::rename` reports the
/// source and destination are on different filesystems, copy-then-delete
/// instead. Used by both rule moves and dedup trashing.
pub fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device_error(&err) => {
            fs::copy(from, to)?;
            fs::remove_file(from)
        }
        Err(err) => Err(err),
    }
}

fn is_cross_device_error(err: &io::Error) -> bool {
    if err.kind() == io::ErrorKind::CrossesDevices {
        return true;
    }
    match err.raw_os_error() {
        Some(18) => true, // EXDEV (Linux/macOS)
        Some(17) => true, // ERROR_NOT_SAME_DEVICE (Windows)
        _ => false,
    }
}

/// Compute the destination path for a file without touching the filesystem.
/// If a file with the same name already exists at the destination, suffix the
/// stem with `_1`, `_2`, ... until a free name is found.
fn resolve_destination(dest_dir: &Path, filename: &str) -> PathBuf {
    let candidate = dest_dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut counter = 1u32;
    loop {
        let next = dest_dir.join(format!("{stem}_{counter}{extension}"));
        if !next.exists() {
            return next;
        }
        counter += 1;
    }
}

fn map_filesystem_error(err: io::Error) -> FluxError {
    if err.kind() == io::ErrorKind::StorageFull {
        return FluxError::Rule(format!("Filesystem is full: {err}"));
    }
    FluxError::Io(err)
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(a), Ok(b)) => a == b,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::engine::{Rule, RuleAction, RulePattern};
    use crate::scanner::metadata::FileEntry;
    use chrono::Utc;
    use tempfile::tempdir;

    fn move_rule(dest: &Path) -> Rule {
        Rule {
            pattern: RulePattern::Extension(vec!["txt".to_string()]),
            destination: dest.to_path_buf(),
            action: RuleAction::Move,
            label: "*.txt".to_string(),
        }
    }

    fn entry_at(path: &Path) -> FileEntry {
        FileEntry {
            path: path.to_path_buf(),
            filename: path.file_name().unwrap().to_string_lossy().into_owned(),
            extension: Some("txt".to_string()),
            size_bytes: 4,
            modified: Utc::now(),
            created: None,
            content_hash: None,
            hash_modified: None,
            is_dir: false,
        }
    }

    #[test]
    fn organize_file_moves_to_destination() {
        let dir = tempdir().expect("tempdir");
        let source = dir.path().join("source.txt");
        let dest_dir = dir.path().join("organized");
        std::fs::write(&source, b"data").expect("write");

        let rule = move_rule(&dest_dir);
        let log_path = dir.path().join("activity.jsonl");
        let result = organize_file(&entry_at(&source), &rule, false, &log_path).expect("organize");

        assert!(matches!(result, OrganizeResult::Moved { .. }));
        assert!(!source.exists());
        assert!(dest_dir.join("source.txt").exists());
    }

    #[test]
    fn organize_file_resolves_name_conflicts() {
        let dir = tempdir().expect("tempdir");
        let dest_dir = dir.path().join("organized");
        std::fs::create_dir_all(&dest_dir).expect("mkdir");
        std::fs::write(dest_dir.join("report.txt"), b"existing").expect("write");

        let source = dir.path().join("report.txt");
        std::fs::write(&source, b"new").expect("write");

        let rule = move_rule(&dest_dir);
        let log_path = dir.path().join("activity.jsonl");
        let result = organize_file(&entry_at(&source), &rule, false, &log_path).expect("organize");

        let dest = match result {
            OrganizeResult::Moved { to, .. } => to,
            other => panic!("expected move, got {other:?}"),
        };
        assert_eq!(dest.file_name().unwrap().to_string_lossy(), "report_1.txt");
        assert!(dest.exists());
    }

    #[test]
    fn map_filesystem_error_detects_storage_full() {
        let err = map_filesystem_error(io::Error::new(
            io::ErrorKind::StorageFull,
            "no space left on device",
        ));
        assert!(matches!(err, FluxError::Rule(_)));
        assert!(err.to_string().contains("full"));
    }

    #[test]
    fn dry_run_does_not_create_destination_directory() {
        let dir = tempdir().expect("tempdir");
        let source = dir.path().join("keep.txt");
        let dest_dir = dir.path().join("never/made");
        std::fs::write(&source, b"x").expect("write");

        let rule = move_rule(&dest_dir);
        let log_path = dir.path().join("activity.jsonl");
        let result = organize_file(&entry_at(&source), &rule, true, &log_path).expect("organize");

        assert!(matches!(result, OrganizeResult::DryRun { .. }));
        assert!(source.exists(), "source must be left in place");
        assert!(
            !dest_dir.exists(),
            "dry-run must not create destination directory"
        );
    }

    #[test]
    fn dry_run_does_not_move_file() {
        let dir = tempdir().expect("tempdir");
        let source = dir.path().join("keep.txt");
        let dest_dir = dir.path().join("out");
        std::fs::write(&source, b"x").expect("write");

        let rule = move_rule(&dest_dir);
        let log_path = dir.path().join("activity.jsonl");
        let result = organize_file(&entry_at(&source), &rule, true, &log_path).expect("organize");

        assert!(matches!(result, OrganizeResult::DryRun { .. }));
        assert!(source.exists());
    }
}
