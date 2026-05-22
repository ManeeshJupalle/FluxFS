//! Clap command definitions for the FluxFS CLI.

use clap::{Parser, Subcommand};

/// Intelligent filesystem autopilot — watch, organize, deduplicate, and search your files.
#[derive(Parser, Debug)]
#[command(name = "flux", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available FluxFS subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// First-time setup: create config and data directories
    Init,
    /// Start the background file watcher daemon
    Start,
    /// Stop the running daemon
    Stop,
    /// Fuzzy search indexed files
    Find {
        /// Search query
        query: String,
    },
    /// Filesystem health overview
    Status,
    /// Show recent activity log
    Log,
    /// Find and handle duplicate files
    Dedup {
        /// Preview actions without modifying files
        #[arg(long)]
        dry_run: bool,
        /// Required when using the delete strategy
        #[arg(long)]
        confirm: bool,
    },
    /// Run organization rules once (no daemon)
    Organize,
    /// Print current config location and contents
    Config,
}

/// Human-readable name for a subcommand (used in stub handlers).
pub fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Init => "init",
        Commands::Start => "start",
        Commands::Stop => "stop",
        Commands::Find { .. } => "find",
        Commands::Status => "status",
        Commands::Log => "log",
        Commands::Dedup { .. } => "dedup",
        Commands::Organize => "organize",
        Commands::Config => "config",
    }
}

/// Stub handler — prints not-implemented message until the command is wired.
pub fn run_stub(command: &Commands) {
    println!("Not implemented yet: {}", command_name(command));
}
