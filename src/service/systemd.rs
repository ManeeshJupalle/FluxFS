//! Linux systemd user unit integration.

use crate::errors::{FluxError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const UNIT_NAME: &str = "fluxfs.service";
const TRAY_UNIT_NAME: &str = "fluxfs-tray.service";

pub fn unit_path() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("systemd/user").join(UNIT_NAME))
        .unwrap_or_else(|| PathBuf::from(".config/systemd/user").join(UNIT_NAME))
}

pub fn tray_unit_path() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("systemd/user").join(TRAY_UNIT_NAME))
        .unwrap_or_else(|| PathBuf::from(".config/systemd/user").join(TRAY_UNIT_NAME))
}

pub fn render_unit(binary: &Path) -> String {
    format!(
        r#"[Unit]
Description=FluxFS filesystem autopilot
After=default.target

[Service]
Type=simple
ExecStart={} start --daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#,
        binary.display()
    )
}

pub fn render_tray_unit(tray: &Path) -> String {
    format!(
        r#"[Unit]
Description=FluxFS system tray
After=fluxfs.service

[Service]
Type=simple
ExecStart={}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#,
        tray.display()
    )
}

pub fn install(binary: &Path, tray: Option<&Path>) -> Result<()> {
    let path = unit_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }
    fs::write(&path, render_unit(binary)).map_err(FluxError::from)?;

    if let Some(tray) = tray {
        if tray.exists() {
            fs::write(tray_unit_path(), render_tray_unit(tray)).map_err(FluxError::from)?;
        }
    }

    run_systemctl(&["--user", "daemon-reload"])?;
    run_systemctl(&["--user", "enable", UNIT_NAME])?;
    if tray.is_some_and(|t| t.exists()) {
        run_systemctl(&["--user", "enable", TRAY_UNIT_NAME])?;
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let _ = stop();
    let _ = run_systemctl(&["--user", "disable", UNIT_NAME]);
    let _ = run_systemctl(&["--user", "disable", TRAY_UNIT_NAME]);

    for path in [unit_path(), tray_unit_path()] {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
    run_systemctl(&["--user", "daemon-reload"])?;
    Ok(())
}

pub fn start() -> Result<()> {
    run_systemctl(&["--user", "start", UNIT_NAME])?;
    let _ = run_systemctl(&["--user", "start", TRAY_UNIT_NAME]);
    Ok(())
}

pub fn stop() -> Result<()> {
    let _ = run_systemctl(&["--user", "stop", TRAY_UNIT_NAME]);
    run_systemctl(&["--user", "stop", UNIT_NAME])
}

fn run_systemctl(args: &[&str]) -> Result<()> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|err| {
            FluxError::Watcher(format!(
                "Failed to run systemctl (is systemd available?): {err}"
            ))
        })?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(FluxError::Watcher(format!(
            "systemctl {} failed: {}",
            args.join(" "),
            stderr.trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn unit_file_contains_binary_and_daemon_flag() {
        let binary = PathBuf::from("/usr/local/bin/flux");
        let unit = render_unit(&binary);
        assert!(unit.contains("ExecStart=/usr/local/bin/flux start --daemon"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn tray_unit_contains_tray_binary() {
        let tray = PathBuf::from("/usr/local/bin/fluxfs-tray");
        let unit = render_tray_unit(&tray);
        assert!(unit.contains("ExecStart=/usr/local/bin/fluxfs-tray"));
    }
}
