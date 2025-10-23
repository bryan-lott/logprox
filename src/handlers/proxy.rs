use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::Response,
};
use crate::config::{ConfigHolder, ResponseCaptureConfig};
use serde_json;
use std::sync::Arc;
use tracing::info;

pub async fn proxy_handler(State(config): State<Arc<ConfigHolder>>, req: Request) -> Response {
    let config = config.get();
    let start_time = std::time::Instant::now();

    // Check if request should be dropped
    if let Some(drop_response) = config.should_drop_request(&req, "") {
        let response = Response::builder()
            .status(drop_response.status_code)
            .body(Body::from(drop_response.body.unwrap_or_default()))
            .unwrap();

        // Check if response should be logged
        if let Some(capture_config) = config.should_log_response(
            response.status().as_u16(),
            response.headers(),
            "" // For dropped responses, no body content
        ) {
            log_response(&req, &response, capture_config, start_time.elapsed());
        }

        return response;
    }

    // For now, just return a simple response
    // In a real proxy, this would forward the request
    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from("Proxied"))
        .unwrap();

    // Check if response should be logged
    if let Some(capture_config) = config.should_log_response(
        response.status().as_u16(),
        response.headers(),
        "Proxied" // Mock response body
    ) {
        log_response(&req, &response, capture_config, start_time.elapsed());
    }

    response
}

fn log_response(
    req: &Request,
    resp: &Response,
    capture_config: &ResponseCaptureConfig,
    duration: std::time::Duration,
) {
    let mut log_entry = serde_json::json!({
        "type": "response",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    if capture_config.status_code {
        log_entry["status_code"] = resp.status().as_u16().into();
    }

    if capture_config.timing {
        log_entry["duration_ms"] = (duration.as_millis() as u64).into();
    }

    if !capture_config.headers.is_empty() {
        let mut headers_obj = serde_json::Map::new();
        for header_name in &capture_config.headers {
            if let Some(value) = resp.headers().get(header_name) {
                if let Ok(value_str) = value.to_str() {
                    headers_obj.insert(header_name.clone(), value_str.into());
                }
            }
        }
        if !headers_obj.is_empty() {
            log_entry["headers"] = headers_obj.into();
        }
    }

    if capture_config.body {
        // For logging purposes, we'll capture the body as string
        // In a real implementation, we'd need to handle streaming bodies
        log_entry["body"] = "Proxied".into(); // Mock body
    }

    // Add request context for correlation
    log_entry["request_method"] = req.method().as_str().into();
    log_entry["request_path"] = req.uri().path().into();

    info!("{}", serde_json::to_string(&log_entry).unwrap_or_else(|_| "Failed to serialize response log".to_string()));
}