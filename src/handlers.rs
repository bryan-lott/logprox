use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use crate::config::{Config, ConfigHolder};
use serde_json;
use std::sync::Arc;

pub async fn get_health_check() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from("OK"))
        .unwrap()
}

pub async fn get_config(State(config): State<Arc<ConfigHolder>>) -> impl IntoResponse {
    let config = config.get();
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        serde_json::to_string_pretty(&*config).unwrap(),
    )
}

pub async fn reload_config(State(config): State<Arc<ConfigHolder>>) -> impl IntoResponse {
    match config.reload() {
        Ok(_) => (StatusCode::OK, "Configuration reloaded successfully".to_string()),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to reload configuration: {}", e),
        ),
    }
}

pub async fn get_config_docs() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        include_str!("../config_docs.md"),
    )
}

pub async fn proxy_handler(State(config): State<Arc<ConfigHolder>>, req: Request) -> Response {
    let config = config.get();

    // Check if request should be dropped
    if let Some(drop_response) = config.should_drop_request(&req, "") {
        return Response::builder()
            .status(drop_response.status_code)
            .body(Body::from(drop_response.body.unwrap_or_default()))
            .unwrap();
    }

    // For now, just return a simple response
    // In a real proxy, this would forward the request
    Response::builder()
        .status(StatusCode::OK)
        .body(Body::from("Proxied"))
        .unwrap()
}