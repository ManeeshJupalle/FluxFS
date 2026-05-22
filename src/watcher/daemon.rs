//! Background daemon — PID file, signals, and watch loop.

use crate::config::FluxConfig;
use crate::config::{ensure_data_dir, watch_rulesets_from_config};
use crate::errors::{FluxError, Result};
use crate::index::{index_file_path, load, save};
use crate::reporting::activity::activity_log_path;
use crate::watcher::handler::FluxWatcher;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::signal;
use tokio::time;
use tracing::{info, warn};

/// PID filename inside the data directory.
pub const PID_FILENAME: &str = "flux.pid";

/// Daemon start timestamp file (RFC3339).
pub const DAEMON_STARTED_FILENAME: &str = "flux.started";

/// Interval between automatic index saves.
const INDEX_SAVE_INTERVAL_SECS: u64 = 300;

/// Path to the daemon PID file.
pub fn pid_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(PID_FILENAME)
}

/// Path to the daemon start-time file.
pub fn daemon_started_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DAEMON_STARTED_FILENAME)
}

/// Record when the daemon started (for uptime in `flux status`).
pub fn write_daemon_started(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }
    let stamp = Utc::now().to_rfc3339();
    fs::write(path, stamp).map_err(FluxError::from)?;
    Ok(())
}

/// Read daemon start time from file.
pub fn read_daemon_started(path: &Path) -> Result<DateTime<Utc>> {
    let contents = fs::read_to_string(path).map_err(FluxError::from)?;
    DateTime::parse_from_rfc3339(contents.trim())
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| FluxError::Watcher(format!("Invalid timestamp in {}", path.display())))
}

/// Remove daemon start-time file if present.
pub fn remove_daemon_started(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).map_err(FluxError::from)?;
    }
    Ok(())
}

/// Write the current process ID to the PID file.
pub fn write_pid_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }
    let pid = std::process::id();
    fs::write(path, pid.to_string()).map_err(FluxError::from)?;
    Ok(())
}

/// Read a PID from file.
pub fn read_pid_file(path: &Path) -> Result<u32> {
    let contents = fs::read_to_string(path).map_err(FluxError::from)?;
    contents
        .trim()
        .parse::<u32>()
        .map_err(|_| FluxError::Watcher(format!("Invalid PID in {}", path.display())))
}

/// Remove the PID file if present.
pub fn remove_pid_file(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).map_err(FluxError::from)?;
    }
    Ok(())
}

/// Returns true if a process with the given PID appears to be running.
pub fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output();
        match output {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                text.contains(&pid.to_string())
            }
            Err(_) => false,
        }
    }

    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

/// Returns true when a PID file exists and refers to a live process.
pub fn is_daemon_running(data_dir: &Path) -> Result<bool> {
    let pid_path = pid_file_path(data_dir);
    if !pid_path.exists() {
        return Ok(false);
    }

    let pid = read_pid_file(&pid_path)?;
    Ok(is_process_alive(pid))
}

/// Stop a running daemon via its PID file.
pub fn stop_daemon(data_dir: &Path) -> Result<()> {
    let pid_path = pid_file_path(data_dir);
    if !pid_path.exists() {
        return Err(FluxError::Watcher(
            "FluxFS daemon is not running (no PID file found).".to_string(),
        ));
    }

    let pid = read_pid_file(&pid_path)?;
    if !is_process_alive(pid) {
        remove_pid_file(&pid_path)?;
        return Err(FluxError::Watcher(
            "FluxFS daemon is not running (stale PID file removed).".to_string(),
        ));
    }

    terminate_process(pid)?;

    // Give the daemon a moment to shut down gracefully.
    for _ in 0..20 {
        if !is_process_alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if is_process_alive(pid) {
        return Err(FluxError::Watcher(format!(
            "Daemon PID {pid} did not stop. Try again or end the process manually."
        )));
    }

    if pid_path.exists() {
        remove_pid_file(&pid_path)?;
    }

    Ok(())
}

fn terminate_process(pid: u32) -> Result<()> {
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(FluxError::from)?;
        if !status.success() {
            return Err(FluxError::Watcher(format!(
                "Failed to stop daemon process {pid}."
            )));
        }
    }

    #[cfg(not(windows))]
    {
        let status = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .map_err(FluxError::from)?;
        if !status.success() {
            return Err(FluxError::Watcher(format!(
                "Failed to send SIGTERM to daemon process {pid}."
            )));
        }
    }

    Ok(())
}

/// Run the daemon event loop until a shutdown signal is received.
pub async fn run_daemon(config: FluxConfig) -> Result<()> {
    let data_dir = ensure_data_dir(&config)?;
    let pid_path = pid_file_path(&data_dir);
    let index_path = index_file_path(&config)?;

    if is_daemon_running(&data_dir)? {
        return Err(FluxError::Watcher(
            "FluxFS daemon is already running. Use `flux stop` first.".to_string(),
        ));
    }

    write_pid_file(&pid_path)?;
    let started_path = daemon_started_path(&data_dir);
    write_daemon_started(&started_path)?;

    let index = Arc::new(Mutex::new(load(&index_path)?));
    let rulesets = watch_rulesets_from_config(&config)?;
    let activity_log = activity_log_path(&data_dir);
    let dry_run = config.general.dry_run;

    let watch_paths: Vec<PathBuf> = rulesets.iter().map(|r| r.watch_path.clone()).collect();

    let (tx, rx) = std::sync::mpsc::channel();
    let flux = FluxWatcher::new(Arc::clone(&index), rulesets, activity_log.clone(), dry_run);
    let watcher = FluxWatcher::build_watcher(tx, &watch_paths)?;

    info!(pid = std::process::id(), "FluxFS daemon started");

    let index_path_clone = index_path.clone();
    let index_for_save = Arc::clone(&index);
    let save_task = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(INDEX_SAVE_INTERVAL_SECS));
        loop {
            interval.tick().await;
            if let Ok(index) = index_for_save.lock() {
                if let Err(err) = save(&index, &index_path_clone) {
                    warn!(error = %err, "Periodic index save failed");
                } else {
                    info!(path = %index_path_clone.display(), "Index saved (periodic)");
                }
            }
        }
    });

    let watcher_task = tokio::task::spawn_blocking(move || flux.run_event_loop(rx, watcher));

    shutdown_signal().await;

    info!("FluxFS daemon shutting down");

    if let Err(err) = watcher_task.await {
        warn!(error = %err, "Watcher task ended unexpectedly");
    }
    save_task.abort();

    if let Ok(index) = index.lock() {
        save(&index, &index_path)?;
        info!(path = %index_path.display(), "Index saved on shutdown");
    }

    remove_pid_file(&pid_path)?;
    remove_daemon_started(&started_path)?;
    info!("FluxFS daemon stopped");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pid_file_round_trip() {
        let dir = tempdir().expect("tempdir");
        let path = pid_file_path(dir.path());
        write_pid_file(&path).expect("write");
        let pid = read_pid_file(&path).expect("read");
        assert_eq!(pid, std::process::id());
        remove_pid_file(&path).expect("remove");
        assert!(!path.exists());
    }

    #[test]
    fn is_process_alive_current_pid() {
        assert!(is_process_alive(std::process::id()));
        assert!(!is_process_alive(0));
    }
}
