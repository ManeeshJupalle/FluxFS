//! Windows logon Run-key integration (Phase A + B tray).

use crate::errors::{FluxError, Result};
use std::path::Path;
use std::process::Command;

const RUN_VALUE_NAME: &str = "FluxFS";
const TRAY_RUN_VALUE_NAME: &str = "FluxFSTray";

pub fn install(binary: &Path, tray: Option<&Path>) -> Result<()> {
    let value = format!("\"{}\" start --daemon", binary.display());
    run_reg(&[
        "add",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        RUN_VALUE_NAME,
        "/t",
        "REG_SZ",
        "/d",
        &value,
        "/f",
    ])?;

    if let Some(tray) = tray {
        if tray.exists() {
            let tray_value = format!("\"{}\"", tray.display());
            run_reg(&[
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                TRAY_RUN_VALUE_NAME,
                "/t",
                "REG_SZ",
                "/d",
                &tray_value,
                "/f",
            ])?;
        }
    }

    Ok(())
}

pub fn uninstall() -> Result<()> {
    let _ = run_reg(&[
        "delete",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        RUN_VALUE_NAME,
        "/f",
    ]);
    let _ = run_reg(&[
        "delete",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
        "/v",
        TRAY_RUN_VALUE_NAME,
        "/f",
    ]);
    Ok(())
}

pub fn start(binary: &Path) -> Result<()> {
    crate::service::spawn_detached_daemon(binary)?;
    if let Ok(tray) = crate::service::tray_binary_path() {
        if tray.exists() {
            let _ = crate::service::spawn_tray(&tray);
        }
    }
    Ok(())
}

pub fn stop() -> Result<()> {
    Ok(())
}

fn run_reg(args: &[&str]) -> Result<()> {
    let output = Command::new("reg")
        .args(args)
        .output()
        .map_err(|err| FluxError::Watcher(format!("Failed to run reg.exe: {err}")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(FluxError::Watcher(format!(
            "reg {} failed: {}",
            args.join(" "),
            stderr.trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn run_value_names_are_stable() {
        assert_eq!(super::RUN_VALUE_NAME, "FluxFS");
        assert_eq!(super::TRAY_RUN_VALUE_NAME, "FluxFSTray");
    }
}
