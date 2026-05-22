//! Shared formatting helpers for CLI output.

use chrono::{DateTime, Local, Utc};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Format byte counts for human-readable display.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Format a duration as uptime (e.g. `3h 22m`).
pub fn format_uptime(duration: Duration) -> String {
    let secs = duration.as_secs();
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let minutes = (secs % 3600) / 60;

    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Format last-scan time relative to today.
pub fn format_last_scan(time: DateTime<Utc>) -> String {
    let local: DateTime<Local> = time.into();
    let now = Local::now();
    let same_day = local.date_naive() == now.date_naive();

    if same_day {
        format!("Today at {}", local.format("%H:%M"))
    } else {
        format!("{}", local.format("%b %d at %H:%M"))
    }
}

/// Shorten a path with `~` when under the user's home directory.
pub fn shorten_path(path: &Path, home: Option<&Path>) -> String {
    let Some(home) = home else {
        return path.display().to_string();
    };

    if path.starts_with(home) {
        let rest = path.strip_prefix(home).unwrap_or(path);
        let suffix = rest.to_string_lossy().replace('\\', "/");
        if suffix.is_empty() || suffix == "/" {
            "~".to_string()
        } else if suffix.starts_with('/') {
            format!("~{suffix}")
        } else {
            format!("~/{suffix}")
        }
    } else {
        path.display().to_string()
    }
}

/// User home directory for path display.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_scales() {
        assert_eq!(format_bytes(500), "500 B");
        assert!(format_bytes(2048).contains("KB"));
    }

    #[test]
    fn format_uptime_hours_and_minutes() {
        assert_eq!(
            format_uptime(Duration::from_secs(3 * 3600 + 22 * 60)),
            "3h 22m"
        );
    }

    #[test]
    fn format_last_scan_today_includes_today() {
        let now = Utc::now();
        let text = format_last_scan(now);
        assert!(text.starts_with("Today at"));
    }

    #[test]
    fn shorten_path_replaces_home_prefix() {
        let home = PathBuf::from("/home/user");
        let path = PathBuf::from("/home/user/Downloads/file.pdf");
        assert_eq!(shorten_path(&path, Some(&home)), "~/Downloads/file.pdf");
    }
}
