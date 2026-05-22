//! File move/copy operations with safety checks.

use crate::errors::{FluxError, Result};
use crate::reporting::activity::log_file_moved;
use crate::rules::engine::{Rule, RuleAction};
use crate::scanner::metadata::FileEntry;
use std::fs;
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
    let dest_path = resolve_destination(&rule.destination, &entry.filename)?;

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

    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            FluxError::Rule(format!(
                "Cannot create destination directory {}: {e}",
                parent.display()
            ))
        })?;
    }

    match rule.action {
        RuleAction::Move => {
            fs::rename(&entry.path, &dest_path).map_err(|e| {
                FluxError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to move {} to {}: {e}",
                        entry.path.display(),
                        dest_path.display()
                    ),
                ))
            })?;
            log_file_moved(activity_log, &entry.path, &dest_path, &rule.label)?;
            Ok(OrganizeResult::Moved {
                from: entry.path.clone(),
                to: dest_path,
            })
        }
        RuleAction::Copy => {
            fs::copy(&entry.path, &dest_path).map_err(|e| {
                FluxError::Io(std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to copy {} to {}: {e}",
                        entry.path.display(),
                        dest_path.display()
                    ),
                ))
            })?;
            log_file_moved(activity_log, &entry.path, &dest_path, &rule.label)?;
            Ok(OrganizeResult::Copied {
                from: entry.path.clone(),
                to: dest_path,
            })
        }
    }
}

fn resolve_destination(dest_dir: &Path, filename: &str) -> Result<PathBuf> {
    fs::create_dir_all(dest_dir).map_err(FluxError::from)?;

    let mut candidate = dest_dir.join(filename);
    if !candidate.exists() {
        return Ok(candidate);
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
        candidate = dest_dir.join(format!("{stem}_{counter}{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
        counter += 1;
    }
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
