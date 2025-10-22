pub mod config;
pub mod handlers;

// Re-export handler functions for testing
pub use handlers::{get_health_check, get_config, get_config_docs, reload_config, proxy_handler};