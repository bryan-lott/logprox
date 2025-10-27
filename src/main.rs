pub mod config;
pub mod handlers;

use axum::{
    routing::{get, post, Router},
};
use config::{Config, ConfigHolder};
use std::sync::Arc;
use tracing::{info, Level};

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .json()
        .init();

    // Load configuration
    let config_file = std::env::var("CONFIG_FILE").unwrap_or_else(|_| "config.yaml".to_string());
    let config = Config::from_file(&config_file).unwrap_or_else(|e| {
        eprintln!("Failed to load config from {}: {}", config_file, e);
        std::process::exit(1);
    });
    let config = Arc::new(ConfigHolder::new(config));

    // Build our application with health check and config route
    let app = Router::new()
        .route("/health", get(handlers::get_health_check))
        .route("/config", get(handlers::get_config))
        .route("/config/docs", get(handlers::get_config_docs))
        .route("/config/reload", post(handlers::reload_config))
        .fallback(handlers::proxy_handler)
        .with_state(config);

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);
    let addr = format!("0.0.0.0:{}", port);
    info!("Starting proxy server on {}", addr);

    // Run it
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
