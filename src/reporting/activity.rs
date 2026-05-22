//! Activity log tracking (append-only JSONL).

use crate::errors::{FluxError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// A single activity log record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityEntry {
    pub timestamp: DateTime<Utc>,
    pub action: ActivityAction,
}

/// Types of actions recorded in the activity log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActivityAction {
    DuplicateFound {
        original: PathBuf,
        duplicate: PathBuf,
        size: u64,
    },
    DuplicateRemoved {
        path: PathBuf,
        size: u64,
    },
    FileMoved {
        from: PathBuf,
        to: PathBuf,
        rule: String,
    },
}

/// Default activity log filename inside the data directory.
pub const ACTIVITY_FILENAME: &str = "activity.jsonl";

/// Path to the activity log for a data directory.
pub fn activity_log_path(data_dir: &Path) -> PathBuf {
    data_dir.join(ACTIVITY_FILENAME)
}

/// Append an entry to the JSONL activity log.
pub fn append(log_path: &Path, entry: ActivityEntry) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }

    let line = serde_json::to_string(&entry).map_err(|e| {
        FluxError::Serialization(format!("Failed to serialize activity entry: {e}"))
    })?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(FluxError::from)?;

    writeln!(file, "{line}").map_err(FluxError::from)?;
    Ok(())
}

/// Log that a duplicate was detected.
pub fn log_duplicate_found(
    log_path: &Path,
    original: &Path,
    duplicate: &Path,
    size: u64,
) -> Result<()> {
    append(
        log_path,
        ActivityEntry {
            timestamp: Utc::now(),
            action: ActivityAction::DuplicateFound {
                original: original.to_path_buf(),
                duplicate: duplicate.to_path_buf(),
                size,
            },
        },
    )
}

/// Log that a file was moved or copied by a rule.
pub fn log_file_moved(log_path: &Path, from: &Path, to: &Path, rule: &str) -> Result<()> {
    append(
        log_path,
        ActivityEntry {
            timestamp: Utc::now(),
            action: ActivityAction::FileMoved {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                rule: rule.to_string(),
            },
        },
    )
}

/// Log that a duplicate was removed or moved to trash.
pub fn log_duplicate_removed(log_path: &Path, path: &Path, size: u64) -> Result<()> {
    append(
        log_path,
        ActivityEntry {
            timestamp: Utc::now(),
            action: ActivityAction::DuplicateRemoved {
                path: path.to_path_buf(),
                size,
            },
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use tempfile::tempdir;

    #[test]
    fn append_and_read_activity_entry() {
        let dir = tempdir().expect("tempdir");
        let log_path = dir.path().join("activity.jsonl");

        log_duplicate_found(
            &log_path,
            Path::new("/tmp/original.txt"),
            Path::new("/tmp/duplicate.txt"),
            128,
        )
        .expect("log");

        let file = fs::File::open(&log_path).expect("open");
        let line = BufReader::new(file).lines().next().unwrap().expect("line");
        let entry: ActivityEntry = serde_json::from_str(&line).expect("parse");
        assert!(matches!(
            entry.action,
            ActivityAction::DuplicateFound { .. }
        ));
    }
}
