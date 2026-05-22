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

/// `flux init` — create config and data directories, write default config if missing.
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

    println!("FluxFS initialized.");
    println!("  Config: {}", config_path.display());
    println!("  Data:   {}", data_dir.display());

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
