//! Duplicate detection and resolution.

pub mod detector;

pub use detector::{build_report, resolve_duplicates, DuplicateReport};
