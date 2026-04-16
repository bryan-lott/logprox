//! # LogProx
//!
//! An HTTP proxy with conditional request logging, request dropping, and response logging,
//! all driven by regex-based rules in a YAML config file.
//!
//! ## Proxy URL format
//!
//! Clients send requests to LogProx with the upstream URL embedded in the path:
//!
//! ```text
//! http://localhost:3000/https://api.example.com/v1/users
//!                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ upstream URL
//! ```
//!
//! ## Embedding
//!
//! LogProx is primarily used as a standalone binary (`cargo run` / `./logprox`), but the
//! axum handlers are public so you can embed it into a larger axum application:
//!
//! ```rust,no_run
//! use axum::{Router, routing::get};
//! use logprox::{config::{Config, ConfigHolder}, proxy_handler, get_health_check};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = Config::from_file("config.yaml").unwrap();
//!     let state = Arc::new(ConfigHolder::new(config));
//!     let app: Router = Router::new()
//!         .route("/health", get(get_health_check))
//!         .fallback(proxy_handler)
//!         .with_state(state);
//!     // axum::serve(listener, app).await.unwrap();
//! }
//! ```
//!
//! ## Configuration
//!
//! See [`config::Config`] and the `/config/docs` endpoint (served by [`get_config_docs`])
//! for full configuration reference.

pub mod config;
pub mod handlers;

pub use handlers::{get_health_check, get_config, get_config_docs, reload_config, proxy_handler};

#[doc(hidden)]
pub use handlers::proxy::{extract_upstream_url, parse_duration_string};
