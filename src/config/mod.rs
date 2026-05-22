//! Configuration loading and types.

pub mod parser;
pub mod rules;
pub mod types;

pub use parser::{
    config_file_path, ensure_data_dir, load_config, load_config_from_path, save_default_config,
};
pub use rules::watch_rulesets_from_config;
pub use types::{DuplicatesConfig, FluxConfig, IndexConfig};
