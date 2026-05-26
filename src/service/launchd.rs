//! macOS LaunchAgent integration.

use crate::errors::{FluxError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const LABEL: &str = "com.fluxfs.daemon";
const PLIST_NAME: &str = "com.fluxfs.daemon.plist";

const TRAY_LABEL: &str = "com.fluxfs.tray";
const TRAY_PLIST_NAME: &str = "com.fluxfs.tray.plist";

pub fn plist_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/LaunchAgents").join(PLIST_NAME))
        .unwrap_or_else(|| PathBuf::from("Library/LaunchAgents").join(PLIST_NAME))
}

pub fn tray_plist_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/LaunchAgents").join(TRAY_PLIST_NAME))
        .unwrap_or_else(|| PathBuf::from("Library/LaunchAgents").join(TRAY_PLIST_NAME))
}

pub fn render_tray_plist(tray: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{tray}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
"#,
        label = TRAY_LABEL,
        tray = xml_escape(&tray.display().to_string()),
    )
}

pub fn render_plist(binary: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>start</string>
        <string>--daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/dev/null</string>
    <key>StandardErrorPath</key>
    <string>/dev/null</string>
</dict>
</plist>
"#,
        label = LABEL,
        binary = xml_escape(&binary.display().to_string()),
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn install(binary: &Path, tray: Option<&Path>) -> Result<()> {
    let path = plist_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(FluxError::from)?;
    }
    fs::write(&path, render_plist(binary)).map_err(FluxError::from)?;
    load_agent(&path)?;

    if let Some(tray) = tray {
        if tray.exists() {
            let tray_path = tray_plist_path();
            fs::write(&tray_path, render_tray_plist(tray)).map_err(FluxError::from)?;
            load_agent(&tray_path)?;
        }
    }

    Ok(())
}

pub fn uninstall() -> Result<()> {
    for path in [plist_path(), tray_plist_path()] {
        if path.exists() {
            let _ = unload_agent(&path);
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

pub fn start() -> Result<()> {
    run_launchctl(&["kickstart", "-k", &service_target()])?;
    let _ = run_launchctl(&["kickstart", "-k", &format!("{}/{}", gui_domain(), TRAY_LABEL)]);
    Ok(())
}

pub fn stop() -> Result<()> {
    let _ = run_launchctl(&["bootout", &format!("{}/{}", gui_domain(), TRAY_LABEL)]);
    run_launchctl(&["bootout", &service_target()])
}

fn load_agent(path: &Path) -> Result<()> {
    run_launchctl(&[
        "bootstrap",
        &gui_domain(),
        &path.display().to_string(),
    ])
}

fn unload_agent(path: &Path) -> Result<()> {
    run_launchctl(&["bootout", &gui_domain(), &path.display().to_string()])
}

fn service_target() -> String {
    format!("{}/{}", gui_domain(), LABEL)
}

fn gui_domain() -> String {
    format!("gui/{}", current_uid())
}

fn current_uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .filter(|uid| !uid.is_empty())
        .unwrap_or_else(|| "501".to_string())
}

fn run_launchctl(args: &[&str]) -> Result<()> {
    let output = Command::new("launchctl")
        .args(args)
        .output()
        .map_err(|err| FluxError::Watcher(format!("Failed to run launchctl: {err}")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(FluxError::Watcher(format!(
            "launchctl {} failed: {}",
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
    fn plist_contains_daemon_args() {
        let binary = PathBuf::from("/usr/local/bin/flux");
        let plist = render_plist(&binary);
        assert!(plist.contains("<string>/usr/local/bin/flux</string>"));
        assert!(plist.contains("<string>--daemon</string>"));
        assert!(plist.contains("com.fluxfs.daemon"));
    }
}
