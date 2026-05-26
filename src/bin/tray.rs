//! FluxFS system tray — Phase B.

use fluxfs::config::{ensure_data_dir, load_config};
use fluxfs::ipc::{is_paused, set_paused};
use fluxfs::service::{flux_binary_path, settings_binary_path, spawn_detached_process};
use fluxfs::watcher::daemon::{daemon_log_path, is_daemon_running};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop};

const ID_SETTINGS: &str = "settings";
const ID_PAUSE: &str = "pause";
const ID_ORGANIZE: &str = "organize";
const ID_OPEN_DATA: &str = "open_data";
const ID_OPEN_LOG: &str = "open_log";
const ID_QUIT: &str = "quit";

fn main() {
    if let Err(err) = run() {
        eprintln!("fluxfs-tray error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cfg = load_config()?;
    let data_dir = ensure_data_dir(&cfg)?;
    let flux_binary = flux_binary_path()?;
    let log_path = daemon_log_path(&data_dir);

    let settings_item = MenuItem::with_id(ID_SETTINGS, "Settings…", true, None);
    let pause_item = MenuItem::with_id(ID_PAUSE, pause_label(&data_dir), true, None);
    let organize_item = MenuItem::with_id(ID_ORGANIZE, "Run organize now", true, None);
    let open_data_item = MenuItem::with_id(ID_OPEN_DATA, "Open data folder", true, None);
    let open_log_item = MenuItem::with_id(ID_OPEN_LOG, "Open daemon log", true, None);
    let quit_item = MenuItem::with_id(ID_QUIT, "Quit tray", true, None);

    let menu = Menu::new();
    menu.append(&settings_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&pause_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&organize_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&open_data_item)?;
    menu.append(&open_log_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;

    let mut tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(tooltip_for_state(&data_dir))
        .with_icon(icon_for_state(&data_dir)?)
        .build()?;

    let event_loop = EventLoop::new()?;
    let mut next_refresh = Instant::now() + Duration::from_secs(2);

    #[allow(deprecated)]
    event_loop.run(move |event, event_loop| {
        if matches!(event, Event::AboutToWait) {
            while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
                match menu_event.id.0.as_str() {
                    ID_SETTINGS => {
                        if let Ok(settings) = settings_binary_path() {
                            if settings.exists() {
                                let _ = spawn_detached_process(&settings, &[]);
                            } else {
                                let _ = Command::new(&flux_binary).arg("settings").spawn();
                            }
                        }
                    }
                    ID_PAUSE => {
                        let paused = is_paused(&data_dir);
                        let _ = set_paused(&data_dir, !paused);
                        pause_item.set_text(pause_label(&data_dir));
                        refresh_tray(&mut tray_icon, &data_dir, &pause_item);
                    }
                    ID_ORGANIZE => {
                        let _ = Command::new(&flux_binary).arg("organize").spawn();
                    }
                    ID_OPEN_DATA => {
                        let _ = open_path(&data_dir);
                    }
                    ID_OPEN_LOG => {
                        let _ = open_path(&log_path);
                    }
                    ID_QUIT => {
                        event_loop.exit();
                    }
                    _ => {}
                }
            }

            while TrayIconEvent::receiver().try_recv().is_ok() {}

            if Instant::now() >= next_refresh {
                refresh_tray(&mut tray_icon, &data_dir, &pause_item);
                next_refresh = Instant::now() + Duration::from_secs(2);
            }
        }

        event_loop.set_control_flow(ControlFlow::WaitUntil(next_refresh));
    })?;

    Ok(())
}

fn refresh_tray(tray_icon: &mut tray_icon::TrayIcon, data_dir: &Path, pause_item: &MenuItem) {
    pause_item.set_text(pause_label(data_dir));
    if let Ok(icon) = icon_for_state(data_dir) {
        let _ = tray_icon.set_icon(Some(icon));
    }
    let _ = tray_icon.set_tooltip(Some(tooltip_for_state(data_dir)));
}

fn pause_label(data_dir: &Path) -> &'static str {
    if is_paused(data_dir) {
        "Resume"
    } else {
        "Pause"
    }
}

fn tooltip_for_state(data_dir: &Path) -> String {
    match (
        is_daemon_running(data_dir).unwrap_or(false),
        is_paused(data_dir),
    ) {
        (true, true) => "FluxFS — Paused".to_string(),
        (true, false) => "FluxFS — Running".to_string(),
        (false, _) => "FluxFS — Daemon stopped".to_string(),
    }
}

fn icon_for_state(data_dir: &Path) -> anyhow::Result<Icon> {
    let running = is_daemon_running(data_dir).unwrap_or(false);
    let paused = is_paused(data_dir);

    let (r, g, b) = if !running {
        (220, 60, 60)
    } else if paused {
        (240, 180, 40)
    } else {
        (60, 180, 90)
    };

    colored_icon(r, g, b)
}

fn colored_icon(r: u8, g: u8, b: u8) -> anyhow::Result<Icon> {
    let size = 16u32;
    let rgba: Vec<u8> = (0..size * size).flat_map(|_| [r, g, b, 255]).collect();
    Icon::from_rgba(rgba, size, size)
        .map_err(|err| anyhow::anyhow!("Failed to build tray icon: {err}"))
}

fn open_path(path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(path).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(path).spawn()?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
    }
    Ok(())
}
