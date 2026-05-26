//! `flux status` dashboard output.

use crate::config::types::expand_tilde;
use crate::config::FluxConfig;
use crate::dedup::build_report;
use crate::errors::Result;
use crate::index::store::FileIndex;
use crate::paths::path_is_under;
use crate::reporting::activity::{rule_hits_for_watch, weekly_summary};
use crate::reporting::format::{
    format_bytes, format_last_scan, format_uptime, home_dir, shorten_path,
};
use crate::service::{service_kind_label, service_status};
use crate::watcher::daemon::{
    daemon_log_path, daemon_started_path, is_daemon_running, pid_file_path, read_daemon_started,
    read_pid_file,
};
use chrono::{Duration as ChronoDuration, Utc};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Render the full status dashboard.
pub fn print_status(
    config: &FluxConfig,
    data_dir: &Path,
    index: &FileIndex,
    activity_log: &Path,
) -> Result<()> {
    let home = home_dir();
    let stats = index.stats();
    let dup_report = build_report(index);
    let weekly = weekly_summary(activity_log)?;
    let watch_paths = config.watch_paths()?;

    println!();
    println!("  {}", "⚡ FluxFS Status".bold().cyan());
    println!(
        "  {}",
        "────────────────────────────────────".bright_black()
    );

    print_daemon_section(data_dir, home.as_deref())?;
    print_index_section(stats, config.watch.len());
    print_weekly_section(&weekly);
    print_attention_section(index, &dup_report, &watch_paths, config)?;
    print_watch_directories(index, activity_log, config, home.as_deref())?;

    Ok(())
}

fn print_daemon_section(data_dir: &Path, home: Option<&Path>) -> Result<()> {
    let running = is_daemon_running(data_dir)?;
    let service = service_status(data_dir)?;

    if running {
        let pid_path = pid_file_path(data_dir);
        let pid = read_pid_file(&pid_path)?;
        let uptime = daemon_uptime(data_dir).unwrap_or(Duration::ZERO);
        let paused = crate::ipc::is_paused(data_dir);
        let state = if paused {
            "● Running (paused)".yellow().to_string()
        } else {
            "● Running".green().to_string()
        };
        println!(
            "  Daemon:      {} (PID {}, uptime {})",
            state,
            pid,
            format_uptime(uptime)
        );
    } else {
        println!("  Daemon:      {}", "○ Stopped".yellow());
    }

    if service.installed {
        let label = service.kind.map(service_kind_label).unwrap_or("registered");
        println!("  Service:     {} (auto-start enabled)", label.green());
    } else {
        println!(
            "  Service:     {} (run `flux install-service` for auto-start)",
            "○ Not installed".bright_black()
        );
    }

    println!(
        "  Daemon log:  {}",
        shorten_path(&daemon_log_path(data_dir), home)
    );

    Ok(())
}

fn daemon_uptime(data_dir: &Path) -> Result<Duration> {
    let started_path = daemon_started_path(data_dir);
    let started = read_daemon_started(&started_path)?;
    let elapsed = Utc::now().signed_duration_since(started);
    Ok(Duration::from_secs(elapsed.num_seconds().max(0) as u64))
}

fn print_index_section(stats: &crate::index::store::IndexStats, watch_count: usize) {
    let scan_secs = stats.scan_duration_ms as f64 / 1000.0;
    println!(
        "  Index:       {} files ({})",
        stats.total_files,
        format_bytes(stats.total_size)
    );
    println!(
        "  Last scan:   {} ({:.1}s)",
        format_last_scan(stats.last_scan),
        scan_secs
    );
    println!("  Watching:    {watch_count} directories");
}

fn print_weekly_section(weekly: &crate::reporting::activity::WeeklySummary) {
    println!();
    println!("  {}", "📊 This Week".bold());
    println!("     Files organized:     {}", weekly.files_organized);
    println!("     Duplicates caught:   {}", weekly.duplicates_caught);
    println!(
        "     Space saved:         {}",
        format_bytes(weekly.space_saved).green()
    );
}

fn print_attention_section(
    index: &FileIndex,
    dup_report: &crate::dedup::detector::DuplicateReport,
    watch_paths: &[PathBuf],
    config: &FluxConfig,
) -> Result<()> {
    let empty_dirs = count_empty_directories(watch_paths)?;
    let old_downloads = count_old_download_files(index, watch_paths, 90);

    if dup_report.duplicate_file_count == 0 && empty_dirs == 0 && old_downloads == 0 {
        return Ok(());
    }

    println!();
    println!("  {}", "⚠️  Attention".bold().yellow());

    if dup_report.duplicate_file_count > 0 {
        println!(
            "     {} duplicates remaining ({} reclaimable)",
            dup_report.duplicate_file_count,
            format_bytes(dup_report.reclaimable_bytes).green()
        );
    }

    if empty_dirs > 0 {
        println!("     {empty_dirs} empty directories found");
    }

    if old_downloads > 0 {
        let downloads = downloads_display_path(watch_paths, config);
        println!("     {old_downloads} files in {downloads} older than 90 days");
    }

    Ok(())
}

fn downloads_display_path(watch_paths: &[PathBuf], config: &FluxConfig) -> String {
    let home = home_dir();
    let downloads = watch_paths.iter().find(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().eq_ignore_ascii_case("downloads"))
            .unwrap_or(false)
    });

    if let Some(path) = downloads {
        return shorten_path(path, home.as_deref());
    }

    config
        .watch
        .first()
        .map(|w| w.path.clone())
        .unwrap_or_else(|| "~/Downloads".to_string())
}

fn print_watch_directories(
    index: &FileIndex,
    activity_log: &Path,
    config: &FluxConfig,
    home: Option<&Path>,
) -> Result<()> {
    println!();
    println!("  {}", "📁 Watched Directories".bold());

    for watch in &config.watch {
        let path = expand_tilde(&watch.path)?;
        let display = shorten_path(&path, home);
        let file_count = index
            .iter_entries()
            .filter(|e| !e.is_dir && path_is_under(&e.path, &path))
            .count();

        let hits = if watch.rules.is_empty() {
            "N/A".to_string()
        } else {
            let moves = rule_hits_for_watch(activity_log, &path)?;
            if file_count == 0 {
                "0%".to_string()
            } else {
                let pct = (moves * 100) / file_count.max(1);
                format!("{pct}%")
            }
        };

        println!(
            "     {:<22} {:>6} files    Rule hits: {}",
            display, file_count, hits
        );
    }

    Ok(())
}

/// Count empty directories under watch roots (non-recursive immediate children only).
fn count_empty_directories(watch_paths: &[PathBuf]) -> Result<usize> {
    let mut count = 0;

    for root in watch_paths {
        if !root.is_dir() {
            continue;
        }
        let entries = fs::read_dir(root).map_err(crate::errors::FluxError::from)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_directory_empty(&path)? {
                count += 1;
            }
        }
    }

    Ok(count)
}

fn is_directory_empty(path: &Path) -> Result<bool> {
    let mut entries = fs::read_dir(path).map_err(crate::errors::FluxError::from)?;
    Ok(entries.next().is_none())
}

fn count_old_download_files(index: &FileIndex, watch_paths: &[PathBuf], days: i64) -> usize {
    let downloads_root = watch_paths.iter().find(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().eq_ignore_ascii_case("downloads"))
            .unwrap_or(false)
    });

    let Some(root) = downloads_root else {
        return 0;
    };

    let cutoff = Utc::now() - ChronoDuration::days(days);
    index
        .iter_entries()
        .filter(|e| !e.is_dir && path_is_under(&e.path, root) && e.modified < cutoff)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::store::FileIndex;
    use crate::reporting::activity::{activity_log_path, log_file_moved, log_scan_completed};
    use crate::scanner::metadata::FileEntry;
    use crate::watcher::daemon::{
        daemon_started_path, pid_file_path, remove_daemon_started, remove_pid_file,
        write_daemon_started, write_pid_file,
    };
    use chrono::Utc;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn sample_config(dir: &Path) -> FluxConfig {
        let mut cfg = FluxConfig::default();
        cfg.general.data_dir = dir.to_str().expect("utf8").to_string();
        cfg.watch[0].path = dir.join("downloads").to_str().expect("utf8").to_string();
        cfg
    }

    #[test]
    fn status_with_no_daemon() {
        let dir = tempdir().expect("tempdir");
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).expect("mkdir");
        let index = FileIndex::new();
        let log = activity_log_path(&data_dir);
        let cfg = sample_config(dir.path());

        assert!(!is_daemon_running(&data_dir).expect("check"));
        print_status(&cfg, &data_dir, &index, &log).expect("print");
    }

    #[test]
    fn status_with_daemon_pid_and_started() {
        let dir = tempdir().expect("tempdir");
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).expect("mkdir");

        write_pid_file(&pid_file_path(&data_dir)).expect("pid");
        write_daemon_started(&daemon_started_path(&data_dir)).expect("started");

        assert!(is_daemon_running(&data_dir).expect("check"));

        let mut index = FileIndex::new();
        index.insert(FileEntry {
            path: dir.path().join("downloads").join("old.pdf"),
            filename: "old.pdf".into(),
            extension: Some("pdf".into()),
            size_bytes: 100,
            modified: Utc::now() - ChronoDuration::days(120),
            created: None,
            content_hash: None,
            hash_modified: None,
            is_dir: false,
        });

        let log = activity_log_path(&data_dir);
        log_scan_completed(&log, 1, 500).expect("scan");
        log_file_moved(
            &log,
            &PathBuf::from("/dl/a.pdf"),
            &PathBuf::from("/dest/a.pdf"),
            "pdf",
        )
        .expect("move");

        let cfg = sample_config(dir.path());
        print_status(&cfg, &data_dir, &index, &log).expect("print");

        remove_pid_file(&pid_file_path(&data_dir)).expect("cleanup pid");
        remove_daemon_started(&daemon_started_path(&data_dir)).expect("cleanup started");
    }

    #[test]
    fn count_empty_directories_finds_empty_child() {
        let dir = tempdir().expect("tempdir");
        let watch = dir.path().join("watch");
        let empty = watch.join("empty_sub");
        fs::create_dir_all(&empty).expect("mkdir");
        fs::write(watch.join("file.txt"), b"x").expect("write");

        let count = count_empty_directories(&[watch]).expect("count");
        assert_eq!(count, 1);
    }
}
