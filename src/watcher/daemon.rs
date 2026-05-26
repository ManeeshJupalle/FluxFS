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
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::signal;
use tokio::time;
use tracing::{info, warn};

#[cfg(not(windows))]
use std::process::Command;

/// PID filename inside the data directory.
pub const PID_FILENAME: &str = "flux.pid";

/// Daemon start timestamp file (RFC3339).
pub const DAEMON_STARTED_FILENAME: &str = "flux.started";

/// Graceful shutdown request file (created by `flux stop`).
pub const SHUTDOWN_FILENAME: &str = "flux.stop";

/// Daemon log file (background mode).
pub const DAEMON_LOG_FILENAME: &str = "flux.log";

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

/// Path to the graceful shutdown request file.
pub fn shutdown_request_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SHUTDOWN_FILENAME)
}

/// Path to the daemon log file.
pub fn daemon_log_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DAEMON_LOG_FILENAME)
}

/// Create a shutdown request file so a running daemon exits gracefully.
pub fn request_daemon_shutdown(data_dir: &Path) -> Result<()> {
    fs::create_dir_all(data_dir).map_err(FluxError::from)?;
    fs::write(shutdown_request_path(data_dir), Utc::now().to_rfc3339()).map_err(FluxError::from)?;
    Ok(())
}

/// Remove a shutdown request file if present.
pub fn clear_shutdown_request(data_dir: &Path) -> Result<()> {
    let path = shutdown_request_path(data_dir);
    if path.exists() {
        fs::remove_file(path).map_err(FluxError::from)?;
    }
    Ok(())
}

/// Returns true when `flux stop` has requested a graceful shutdown.
pub fn shutdown_requested(data_dir: &Path) -> bool {
    shutdown_request_path(data_dir).exists()
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
        // Use the Win32 API directly: shelling out to `tasklist` is unreliable
        // (it can return ERROR: Access denied under certain security contexts).
        // PROCESS_QUERY_LIMITED_INFORMATION succeeds for any process the caller
        // can see, including those owned by other users (when permitted).
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                false
            } else {
                CloseHandle(handle);
                true
            }
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

    request_daemon_shutdown(data_dir)?;

    // Give the daemon time to save the index and exit cleanly.
    for _ in 0..50 {
        if !is_process_alive(pid) {
            clear_shutdown_request(data_dir)?;
            if pid_path.exists() {
                remove_pid_file(&pid_path)?;
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if is_process_alive(pid) {
        warn!(pid, "Graceful shutdown timed out; sending hard terminate");
        terminate_process(pid)?;

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
    }

    clear_shutdown_request(data_dir)?;
    if pid_path.exists() {
        remove_pid_file(&pid_path)?;
    }

    Ok(())
}

fn terminate_process(pid: u32) -> Result<()> {
    #[cfg(windows)]
    {
        // Use the Win32 API directly instead of shelling to `taskkill`, which
        // can fail under restricted security contexts.
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, TerminateProcess, PROCESS_TERMINATE,
        };

        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if handle.is_null() {
                return Err(FluxError::Watcher(format!(
                    "Cannot open daemon process {pid} for termination (already exited or access denied)."
                )));
            }
            let ok = TerminateProcess(handle, 0);
            CloseHandle(handle);
            if ok == 0 {
                return Err(FluxError::Watcher(format!(
                    "Failed to terminate daemon process {pid}."
                )));
            }
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

    clear_shutdown_request(&data_dir)?;
    write_pid_file(&pid_path)?;
    let started_path = daemon_started_path(&data_dir);
    write_daemon_started(&started_path)?;

    let index = Arc::new(Mutex::new(load(&index_path)?));
    let rulesets = watch_rulesets_from_config(&config)?;
    let activity_log = activity_log_path(&data_dir);
    let dry_run = config.general.dry_run;

    let watch_paths: Vec<PathBuf> = rulesets.iter().map(|r| r.watch_path.clone()).collect();

    let (tx, rx) = std::sync::mpsc::channel();
    let flux = FluxWatcher::new(
        Arc::clone(&index),
        rulesets,
        activity_log.clone(),
        data_dir.clone(),
        dry_run,
    );
    let shutdown = flux.shutdown_handle();
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

    let data_dir_for_shutdown = data_dir.clone();
    let shutdown_poll = async move {
        loop {
            time::sleep(Duration::from_millis(500)).await;
            if shutdown_requested(&data_dir_for_shutdown) {
                break;
            }
        }
    };

    tokio::select! {
        () = shutdown_signal() => {},
        () = shutdown_poll => {},
    }

    info!("FluxFS daemon shutting down");

    // Signal the watcher's blocking loop to exit. Without this, awaiting the
    // task hangs forever — the loop's channel sender is owned by the watcher
    // inside the same task, so the channel never disconnects on its own.
    shutdown.store(true, Ordering::Relaxed);

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
    clear_shutdown_request(&data_dir)?;
    info!("FluxFS daemon stopped");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = signal::ctrl_c().await {
            warn!(error = %err, "Failed to listen for Ctrl+C");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(err) => warn!(error = %err, "Failed to listen for SIGTERM"),
        }
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
    fn shutdown_request_file_triggers_shutdown_flag() {
        let dir = tempdir().expect("tempdir");
        assert!(!shutdown_requested(dir.path()));
        request_daemon_shutdown(dir.path()).expect("request");
        assert!(shutdown_requested(dir.path()));
        clear_shutdown_request(dir.path()).expect("clear");
        assert!(!shutdown_requested(dir.path()));
    }
}
