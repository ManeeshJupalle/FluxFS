//! FluxFS library — shared engine, daemon, tray IPC, and service integration.

pub mod cli;
pub mod config;
pub mod dedup;
pub mod errors;
pub mod hasher;
pub mod index;
pub mod ipc;
pub mod paths;
pub mod reporting;
pub mod rules;
pub mod scanner;
pub mod service;
pub mod watcher;
