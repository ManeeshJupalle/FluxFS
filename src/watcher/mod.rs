//! File watcher daemon.

pub mod daemon;
pub mod debounce;
pub mod handler;

pub use daemon::{is_daemon_running, run_daemon, stop_daemon};
