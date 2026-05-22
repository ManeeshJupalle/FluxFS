//! File index — in-memory store, persistence, search.

pub mod persistence;
pub mod search;
pub mod store;

pub use persistence::{index_file_path, load, save};
pub use store::FileIndex;
