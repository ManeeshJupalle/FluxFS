//! Activity log tracking (append-only JSONL).

use crate::errors::{FluxError, Result};
use crate::reporting::format::{format_bytes, home_dir, shorten_path};
use chrono::{DateTime, Local, Utc};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Maximum activity log size before rotation (10 MB).
pub const ACTIVITY_ROTATE_BYTES: u64 = 10 * 1024 * 1024;

/// Default number of entries shown by `flux log`.
pub const DEFAULT_LOG_LIMIT: usize = 10;

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
    FileMoved {
        from: PathBuf,
        to: PathBuf,
        rule: String,
    },
    DuplicateFound {
        original: PathBuf,
        duplicate: PathBuf,
        size: u64,
    },
    DuplicateRemoved {
        path: PathBuf,
        size: u64,
    },
    FileIndexed {
        path: PathBuf,
    },
    FileRemoved {
        path: PathBuf,
    },
    ScanCompleted {
        file_count: usize,
        duration_ms: u64,
    },
}

/// Default activity log filename inside the data directory.
pub const ACTIVITY_FILENAME: &str = "activity.jsonl";

/// Path to the activity log for a data directory.
pub fn activity_log_path(data_dir: &Path) -> PathBuf {
    data_dir.join(ACTIVITY_FILENAME)
}

/// Filter options for reading the activity log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogFilter {
    /// Maximum entries to return (`None` = no limit).
    pub limit: Option<usize>,
    /// Only include entries from today (local timezone).
    pub today_only: bool,
}

impl Default for LogFilter {
    fn default() -> Self {
        Self {
            limit: Some(DEFAULT_LOG_LIMIT),
            today_only: false,
        }
    }
}

/// Append an entry to the JSONL activity log.
pub fn append(log_path: &Path, entry: ActivityEntry) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }

    maybe_rotate(log_path)?;

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

/// Rotate the log when it exceeds [`ACTIVITY_ROTATE_BYTES`].
fn maybe_rotate(log_path: &Path) -> Result<()> {
    let metadata = match fs::metadata(log_path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(FluxError::from(err)),
    };

    if metadata.len() <= ACTIVITY_ROTATE_BYTES {
        return Ok(());
    }

    let backup = log_path.with_extension("jsonl.old");
    if backup.exists() {
        fs::remove_file(&backup).map_err(FluxError::from)?;
    }
    fs::rename(log_path, &backup).map_err(FluxError::from)?;
    Ok(())
}

/// Read activity entries, newest first after filtering.
pub fn read_entries(log_path: &Path, filter: &LogFilter) -> Result<Vec<ActivityEntry>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(log_path).map_err(FluxError::from)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    let today = Local::now().date_naive();

    for line in reader.lines() {
        let line = line.map_err(FluxError::from)?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: ActivityEntry = match serde_json::from_str(&line) {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        if filter.today_only {
            let local: DateTime<Local> = entry.timestamp.into();
            if local.date_naive() != today {
                continue;
            }
        }

        entries.push(entry);
    }

    entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));

    if let Some(limit) = filter.limit {
        entries.truncate(limit);
    }

    Ok(entries)
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

/// Log that a file was added to the index.
pub fn log_file_indexed(log_path: &Path, path: &Path) -> Result<()> {
    append(
        log_path,
        ActivityEntry {
            timestamp: Utc::now(),
            action: ActivityAction::FileIndexed {
                path: path.to_path_buf(),
            },
        },
    )
}

/// Log that a file was removed from the index.
pub fn log_file_removed(log_path: &Path, path: &Path) -> Result<()> {
    append(
        log_path,
        ActivityEntry {
            timestamp: Utc::now(),
            action: ActivityAction::FileRemoved {
                path: path.to_path_buf(),
            },
        },
    )
}

/// Log completion of a full index scan.
pub fn log_scan_completed(log_path: &Path, file_count: usize, duration_ms: u64) -> Result<()> {
    append(
        log_path,
        ActivityEntry {
            timestamp: Utc::now(),
            action: ActivityAction::ScanCompleted {
                file_count,
                duration_ms,
            },
        },
    )
}

/// Weekly activity aggregates from log entries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WeeklySummary {
    pub files_organized: usize,
    pub duplicates_caught: usize,
    pub space_saved: u64,
}

/// Summarize activity from the last seven days.
pub fn weekly_summary(log_path: &Path) -> Result<WeeklySummary> {
    let filter = LogFilter {
        limit: None,
        today_only: false,
    };
    let entries = read_entries(log_path, &filter)?;
    let cutoff = Utc::now() - chrono::Duration::days(7);
    let mut summary = WeeklySummary::default();

    for entry in entries {
        if entry.timestamp < cutoff {
            continue;
        }
        match entry.action {
            ActivityAction::FileMoved { .. } => summary.files_organized += 1,
            ActivityAction::DuplicateFound { .. } => summary.duplicates_caught += 1,
            ActivityAction::DuplicateRemoved { size, .. } => {
                summary.space_saved += size;
            }
            _ => {}
        }
    }

    Ok(summary)
}

/// Count file-move actions under a watch directory (all time).
pub fn rule_hits_for_watch(log_path: &Path, watch_root: &Path) -> Result<usize> {
    let filter = LogFilter {
        limit: None,
        today_only: false,
    };
    let entries = read_entries(log_path, &filter)?;
    let count = entries
        .iter()
        .filter(|entry| {
            matches!(
                &entry.action,
                ActivityAction::FileMoved { from, .. } if from.starts_with(watch_root)
            )
        })
        .count();
    Ok(count)
}

/// Print formatted activity log output.
pub fn print_activity_log(entries: &[ActivityEntry], show_all_hint: bool) {
    let home = home_dir();

    if entries.is_empty() {
        println!();
        println!("  No activity recorded yet.");
        return;
    }

    println!();
    for entry in entries {
        println!("  {}", format_entry_line(entry, home.as_deref()));
    }

    if show_all_hint {
        println!();
        println!(
            "  {}",
            format!(
                "Showing last {} entries. Use --all for full log.",
                entries.len()
            )
            .bright_black()
        );
    }
}

fn format_entry_line(entry: &ActivityEntry, home: Option<&Path>) -> String {
    let ts: DateTime<Local> = entry.timestamp.into();
    let stamp = ts.format("%b %d %H:%M").to_string();

    let body = match &entry.action {
        ActivityAction::FileMoved { from, to, .. } => {
            let name = from
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| from.display().to_string());
            let dest = shorten_path(to.parent().unwrap_or(to.as_path()), home);
            format!("📂 Moved {name} → {dest}")
        }
        ActivityAction::DuplicateRemoved { path, size } => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            format!(
                "🗑  Duplicate removed: {name} ({})",
                format_bytes(*size).green()
            )
        }
        ActivityAction::DuplicateFound {
            duplicate, size, ..
        } => {
            let name = duplicate
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| duplicate.display().to_string());
            format!("🔁 Duplicate found: {name} ({})", format_bytes(*size))
        }
        ActivityAction::FileIndexed { path } => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            format!("📄 Indexed {name}")
        }
        ActivityAction::FileRemoved { path } => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            format!("📄 Removed from index: {name}")
        }
        ActivityAction::ScanCompleted {
            file_count,
            duration_ms,
        } => {
            let secs = *duration_ms as f64 / 1000.0;
            format!("🔍 Full scan completed: {file_count} files indexed in {secs:.1}s")
        }
    };

    format!("[{stamp}] {body}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_entry(log_path: &Path, entry: ActivityEntry) {
        append(log_path, entry).expect("append");
    }

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

        let entries = read_entries(&log_path, &LogFilter::default()).expect("read");
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].action,
            ActivityAction::DuplicateFound { .. }
        ));
    }

    #[test]
    fn read_respects_limit() {
        let dir = tempdir().expect("tempdir");
        let log_path = dir.path().join("activity.jsonl");

        for i in 0..5 {
            write_entry(
                &log_path,
                ActivityEntry {
                    timestamp: Utc.with_ymd_and_hms(2026, 5, 21, 10, i, 0).unwrap(),
                    action: ActivityAction::FileIndexed {
                        path: PathBuf::from(format!("/tmp/file{i}.txt")),
                    },
                },
            );
        }

        let filter = LogFilter {
            limit: Some(2),
            today_only: false,
        };
        let entries = read_entries(&log_path, &filter).expect("read");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].timestamp > entries[1].timestamp);
    }

    #[test]
    fn filter_today_excludes_older_entries() {
        let dir = tempdir().expect("tempdir");
        let log_path = dir.path().join("activity.jsonl");

        write_entry(
            &log_path,
            ActivityEntry {
                timestamp: Utc::now(),
                action: ActivityAction::ScanCompleted {
                    file_count: 10,
                    duration_ms: 100,
                },
            },
        );
        write_entry(
            &log_path,
            ActivityEntry {
                timestamp: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                action: ActivityAction::ScanCompleted {
                    file_count: 1,
                    duration_ms: 50,
                },
            },
        );

        let filter = LogFilter {
            limit: None,
            today_only: true,
        };
        let entries = read_entries(&log_path, &filter).expect("read");
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].action,
            ActivityAction::ScanCompleted { file_count: 10, .. }
        ));
    }

    #[test]
    fn weekly_summary_counts_moves_and_duplicates() {
        let dir = tempdir().expect("tempdir");
        let log_path = dir.path().join("activity.jsonl");

        write_entry(
            &log_path,
            ActivityEntry {
                timestamp: Utc::now(),
                action: ActivityAction::FileMoved {
                    from: PathBuf::from("/tmp/a.pdf"),
                    to: PathBuf::from("/dest/a.pdf"),
                    rule: "pdf".into(),
                },
            },
        );
        write_entry(
            &log_path,
            ActivityEntry {
                timestamp: Utc::now(),
                action: ActivityAction::DuplicateRemoved {
                    path: PathBuf::from("/tmp/dup.pdf"),
                    size: 1024,
                },
            },
        );

        let summary = weekly_summary(&log_path).expect("summary");
        assert_eq!(summary.files_organized, 1);
        assert_eq!(summary.space_saved, 1024);
    }

    #[test]
    fn rotate_when_log_exceeds_limit() {
        let dir = tempdir().expect("tempdir");
        let log_path = dir.path().join("activity.jsonl");

        let mut file = File::create(&log_path).expect("create");
        let padding = "x".repeat((ACTIVITY_ROTATE_BYTES + 1) as usize);
        writeln!(file, "{padding}").expect("write");
        drop(file);

        append(
            &log_path,
            ActivityEntry {
                timestamp: Utc::now(),
                action: ActivityAction::FileIndexed {
                    path: PathBuf::from("/tmp/new.txt"),
                },
            },
        )
        .expect("append");

        assert!(log_path.exists());
        assert!(dir.path().join("activity.jsonl.old").exists());
    }
}
