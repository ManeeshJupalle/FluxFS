//! FluxFS — intelligent filesystem autopilot CLI entry point.

mod cli;
mod config;
mod dedup;
mod errors;
mod hasher;
mod index;
mod reporting;
mod rules;
mod scanner;
mod watcher;

use anyhow::Context;
use clap::Parser;
use cli::commands::{Cli, Commands};
use config::{
    config_file_path, ensure_data_dir, load_config, load_config_from_path, save_default_config,
    FluxConfig,
};
use errors::FluxError;
use index::{index_file_path, load, save, FileIndex};
use scanner::scan_directories;
use std::process;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err:#}");
        process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => run_init()?,
        Commands::Config => run_config()?,
        Commands::Start
        | Commands::Stop
        | Commands::Find { .. }
        | Commands::Status
        | Commands::Log
        | Commands::Dedup
        | Commands::Organize => {
            let cfg = load_config()?;
            init_logging(&cfg)?;
            log_startup(&cfg)?;
            cli::commands::run_stub(&cli.command);
        }
    }

    Ok(())
}

/// Initialize tracing subscriber from config log level (overridable via `RUST_LOG`).
fn init_logging(config: &FluxConfig) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(config.general.log_level.as_str()))
        .context("Invalid log level in config or RUST_LOG environment variable")?;

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {e}"))?;

    Ok(())
}

/// Log config path and watched directories at debug level on startup.
fn log_startup(config: &FluxConfig) -> anyhow::Result<()> {
    let config_path = config_file_path()?;
    debug!(path = %config_path.display(), "Config file location");

    let watch_paths = config.watch_paths()?;
    for path in &watch_paths {
        debug!(watch = %path.display(), "Watching directory");
    }

    Ok(())
}

/// `flux init` — create config/data dirs, scan watch paths, build and save index.
fn run_init() -> anyhow::Result<()> {
    let config_path = config_file_path()?;

    if config_path.exists() {
        let cfg = load_config_from_path(&config_path)?;
        init_logging(&cfg)?;
        info!(path = %config_path.display(), "Config already exists");
    } else {
        let path = save_default_config()?;
        let cfg = load_config_from_path(&path)?;
        init_logging(&cfg)?;
        info!(path = %path.display(), "Created default config");
    }

    let cfg = load_config()?;
    log_startup(&cfg)?;

    let data_dir = ensure_data_dir(&cfg)?;
    info!(path = %data_dir.display(), "Data directory ready");

    let watch_paths = cfg.watch_paths()?;
    info!(directories = watch_paths.len(), "Starting filesystem scan");

    let (entries, summary) = scan_directories(&watch_paths, &cfg.index)?;
    let index = FileIndex::from_entries(entries, summary.duration_ms);

    let index_path = index_file_path(&cfg)?;
    save(&index, &index_path)?;
    let loaded = load(&index_path)?;
    debug!(
        path = %index_path.display(),
        files = loaded.len(),
        "Index verified after save"
    );
    info!(
        path = %index_path.display(),
        files = index.len(),
        "Index saved"
    );

    println!("FluxFS initialized.");
    println!("  Config:      {}", config_path.display());
    println!("  Data:        {}", data_dir.display());
    println!("  Index:       {}", index_path.display());
    println!();
    println!("  Scan summary");
    println!("  ────────────────────────────────────");
    println!("  Directories: {}", summary.directories_scanned);
    println!("  Files:       {}", summary.file_count);
    println!("  Total size:  {}", format_bytes(summary.total_size_bytes));
    println!("  Duration:    {:.2}s", summary.duration_ms as f64 / 1000.0);

    Ok(())
}

/// `flux config` — load and pretty-print the current configuration.
fn run_config() -> anyhow::Result<()> {
    let config_path = config_file_path()?;
    let cfg = load_config()?;
    init_logging(&cfg)?;
    log_startup(&cfg)?;

    println!("Config file: {}", config_path.display());
    println!();

    let toml = toml::to_string_pretty(&cfg)
        .map_err(|e| FluxError::Config(format!("Failed to serialize config for display: {e}")))?;

    print!("{toml}");

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
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
