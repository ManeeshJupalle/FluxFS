//! File scanner — directory traversal and metadata.

pub mod metadata;
pub mod walker;

pub use walker::scan_directories;
