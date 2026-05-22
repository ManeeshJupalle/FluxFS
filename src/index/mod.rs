//! File index — in-memory store, persistence, search.

pub mod display;
pub mod persistence;
pub mod search;
pub mod store;

pub use persistence::{index_file_path, load, save};
pub use search::{search, SearchOptions, SortMode};
pub use store::FileIndex;
