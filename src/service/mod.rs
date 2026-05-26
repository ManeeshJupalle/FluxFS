//! OS service registration and background daemon spawning (Phase A).

use crate::errors::{FluxError, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(target_os = "linux")]
mod systemd;
#[cfg(target_os = "macos")]
mod launchd;
#[cfg(windows)]
mod windows;

/// Marker file recording how the background agent was registered.
pub const SERVICE_MARKER_FILENAME: &str = "service.installed";

/// Describes a registered OS integration, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Systemd,
    Launchd,
    WindowsRun,
}

/// Current service registration and daemon state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub installed: bool,
    pub kind: Option<ServiceKind>,
}

/// Returns true when an OS auto-start integration is registered.
pub fn is_service_installed(data_dir: &Path) -> bool {
    service_marker_path(data_dir).exists()
}

/// Read service registration metadata from the data directory.
pub fn service_status(data_dir: &Path) -> Result<ServiceStatus> {
    let marker = service_marker_path(data_dir);
    if !marker.exists() {
        return Ok(ServiceStatus {
            installed: false,
            kind: None,
        });
    }

    let contents = std::fs::read_to_string(&marker).unwrap_or_default();
    let kind = match contents.trim() {
        "systemd" => Some(ServiceKind::Systemd),
        "launchd" => Some(ServiceKind::Launchd),
        "windows-run" => Some(ServiceKind::WindowsRun),
        _ => None,
    };

    Ok(ServiceStatus {
        installed: true,
        kind,
    })
}

/// Register FluxFS to start at login and enable the integration.
pub fn install_service(binary: &Path, data_dir: &Path) -> Result<()> {
    let tray = tray_binary_path().ok();
    platform_install_with_tray(binary, tray.as_deref())?;
    write_service_marker(data_dir, platform_kind())?;
    Ok(())
}

/// Remove OS auto-start registration (does not delete config or index).
pub fn uninstall_service(data_dir: &Path) -> Result<()> {
    platform_uninstall()?;
    remove_service_marker(data_dir)?;
    Ok(())
}

/// Start the daemon via the platform integration (systemd / launchctl / Run key spawn).
pub fn start_service(binary: &Path) -> Result<()> {
    platform_start(binary)
}

/// Stop the daemon via the platform integration when applicable.
pub fn stop_service() -> Result<()> {
    platform_stop()
}

/// Resolve the settings GUI binary next to the current executable.
pub fn settings_binary_path() -> Result<PathBuf> {
    sibling_binary("fluxfs-settings")
}

/// Spawn a detached background `flux start --daemon` process (no OS service registered).
pub fn spawn_detached_daemon(binary: &Path) -> Result<()> {
    spawn_detached(binary, &["start", "--daemon"])
}

/// Spawn a detached process with custom arguments (e.g. settings GUI).
pub fn spawn_detached_process(binary: &Path, args: &[&str]) -> Result<()> {
    spawn_detached(binary, args)
}

/// Resolve the tray binary next to the current executable.
pub fn tray_binary_path() -> Result<PathBuf> {
    sibling_binary("fluxfs-tray")
}

/// Resolve the main FluxFS CLI binary next to the current executable.
pub fn flux_binary_path() -> Result<PathBuf> {
    for name in ["flux", "fluxfs"] {
        let path = sibling_binary(name)?;
        if path.exists() {
            return Ok(path);
        }
    }
    sibling_binary("flux")
}

/// Spawn the tray app detached (used after service install).
pub fn spawn_tray(tray: &Path) -> Result<()> {
    spawn_detached(tray, &[])
}

fn sibling_binary(name: &str) -> Result<PathBuf> {
    let current = std::env::current_exe().map_err(FluxError::from)?;
    let parent = current.parent().ok_or_else(|| {
        FluxError::Watcher("Cannot resolve executable directory.".to_string())
    })?;
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    Ok(parent.join(file))
}

fn spawn_detached(binary: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new(binary);
    cmd.args(args);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn().map_err(|err| {
        FluxError::Watcher(format!(
            "Failed to spawn background process ({}): {err}",
            binary.display()
        ))
    })?;

    Ok(())
}

fn service_marker_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SERVICE_MARKER_FILENAME)
}

fn write_service_marker(data_dir: &Path, kind: ServiceKind) -> Result<()> {
    std::fs::create_dir_all(data_dir).map_err(FluxError::from)?;
    let label = match kind {
        ServiceKind::Systemd => "systemd",
        ServiceKind::Launchd => "launchd",
        ServiceKind::WindowsRun => "windows-run",
    };
    std::fs::write(service_marker_path(data_dir), label).map_err(FluxError::from)?;
    Ok(())
}

fn remove_service_marker(data_dir: &Path) -> Result<()> {
    let path = service_marker_path(data_dir);
    if path.exists() {
        std::fs::remove_file(path).map_err(FluxError::from)?;
    }
    Ok(())
}

fn platform_kind() -> ServiceKind {
    #[cfg(target_os = "linux")]
    {
        ServiceKind::Systemd
    }
    #[cfg(target_os = "macos")]
    {
        ServiceKind::Launchd
    }
    #[cfg(windows)]
    {
        ServiceKind::WindowsRun
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        ServiceKind::Systemd
    }
}

fn platform_install_with_tray(binary: &Path, tray: Option<&Path>) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        systemd::install(binary, tray)
    }
    #[cfg(target_os = "macos")]
    {
        launchd::install(binary, tray)
    }
    #[cfg(windows)]
    {
        windows::install(binary, tray)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = (binary, tray);
        Err(FluxError::Watcher(
            "Service install is not supported on this platform.".to_string(),
        ))
    }
}

fn platform_uninstall() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        systemd::uninstall()
    }
    #[cfg(target_os = "macos")]
    {
        launchd::uninstall()
    }
    #[cfg(windows)]
    {
        windows::uninstall()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        Err(FluxError::Watcher(
            "Service uninstall is not supported on this platform.".to_string(),
        ))
    }
}

fn platform_start(binary: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        systemd::start()
    }
    #[cfg(target_os = "macos")]
    {
        launchd::start()
    }
    #[cfg(windows)]
    {
        windows::start(binary)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = binary;
        spawn_detached_daemon(binary)
    }
}

fn platform_stop() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        systemd::stop()
    }
    #[cfg(target_os = "macos")]
    {
        launchd::stop()
    }
    #[cfg(windows)]
    {
        windows::stop()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        Ok(())
    }
}

/// Human-readable label for status output.
pub fn service_kind_label(kind: ServiceKind) -> &'static str {
    match kind {
        ServiceKind::Systemd => "systemd user service",
        ServiceKind::Launchd => "LaunchAgent",
        ServiceKind::WindowsRun => "Windows logon task",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn service_marker_round_trip() {
        let dir = tempdir().expect("tempdir");
        write_service_marker(dir.path(), ServiceKind::Systemd).expect("write");
        let status = service_status(dir.path()).expect("status");
        assert!(status.installed);
        assert_eq!(status.kind, Some(ServiceKind::Systemd));
        remove_service_marker(dir.path()).expect("remove");
        assert!(!is_service_installed(dir.path()));
    }
}
