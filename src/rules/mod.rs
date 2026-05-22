//! Rule engine — matching and file operations.

pub mod actions;
pub mod engine;
pub mod matcher;

pub use engine::{organize_index, OrganizeSummary};
