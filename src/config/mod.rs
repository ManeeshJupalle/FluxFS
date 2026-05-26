//! Configuration loading and types.

pub mod parser;
pub mod rules;
pub mod size;
pub mod types;

pub use parser::{
    config_file_path, ensure_data_dir, load_config, load_config_from_path, save_config_to_path,
    save_default_config, save_user_config,
};
pub use rules::watch_rulesets_from_config;
pub use types::{DuplicatesConfig, FluxConfig, IndexConfig, WatchConfig, WatchRule};
