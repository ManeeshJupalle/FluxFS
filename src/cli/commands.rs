//! Clap command definitions for the FluxFS CLI.

use crate::index::SortMode;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
/// Sort order for search results.
pub enum SortArg {
    #[default]
    Relevance,
    Size,
    Date,
}

impl From<SortArg> for SortMode {
    fn from(value: SortArg) -> Self {
        match value {
            SortArg::Relevance => SortMode::Relevance,
            SortArg::Size => SortMode::Size,
            SortArg::Date => SortMode::Date,
        }
    }
}

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
    Start {
        /// Run the watcher in the foreground (required in v0.1)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Fuzzy search indexed files
    Find {
        /// Search query
        query: String,
        /// Match against the full path instead of filename only
        #[arg(long)]
        path: bool,
        /// Use glob matching instead of fuzzy search (e.g. "*.pdf")
        #[arg(long)]
        exact: bool,
        /// Filter results by file extension (e.g. pdf)
        #[arg(long)]
        ext: Option<String>,
        /// Sort results by size, date, or relevance (default)
        #[arg(long, value_enum, default_value_t = SortArg::Relevance)]
        sort: SortArg,
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
    Organize {
        /// Preview moves without modifying files
        #[arg(long)]
        dry_run: bool,
    },
    /// Print current config location and contents
    Config,
}

/// Human-readable name for a subcommand (used in stub handlers).
pub fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Init => "init",
        Commands::Start { .. } => "start",
        Commands::Stop => "stop",
        Commands::Find { .. } => "find",
        Commands::Status => "status",
        Commands::Log => "log",
        Commands::Dedup { .. } => "dedup",
        Commands::Organize { .. } => "organize",
        Commands::Config => "config",
    }
}

/// Stub handler — prints not-implemented message until the command is wired.
pub fn run_stub(command: &Commands) {
    println!("Not implemented yet: {}", command_name(command));
}
