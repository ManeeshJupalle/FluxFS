//! FluxFS CLI binary entry point.

use std::process;

fn main() {
    if let Err(err) = fluxfs::cli::runner::run() {
        eprintln!("Error: {err:#}");
        if let Some(hint) = fluxfs::errors::hint_for_anyhow(&err) {
            eprintln!("Hint: {hint}");
        }
        process::exit(1);
    }
}
