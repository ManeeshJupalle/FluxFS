//! FluxFS settings GUI entry point.

fn main() {
    if let Err(err) = fluxfs::gui::run_settings_app() {
        eprintln!("fluxfs-settings error: {err:#}");
        std::process::exit(1);
    }
}
