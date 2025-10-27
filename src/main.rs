/*
TODO's
- [x] Add a health check endpoint
- [x] Add configuration by config file or environment variables
- [x] Add dropping requests based on config
   - [x] Headers
   - [ ] Body
   - [x] Request method
   - [x] Request path
- [ ] Add injection of additional headers based on config
- [x] Add reloading the config file on a POST
- [x] Add a get endpoint for returning the current config
- [ ] Add a get endpoint for returning the config schema
- [ ] Add a config documentation endpoint
*/

mod config;

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, Router},
};
use config::{Config, ConfigHolder};
use serde_json;
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
    let config = Config::from_file("config.yaml").unwrap_or_else(|e| {
        eprintln!("Failed to load config: {}", e);
        std::process::exit(1);
    });
    let config = Arc::new(ConfigHolder::new(config));

    // Build our application with health check and config route
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/config", get(get_config))
        .route("/config/reload", post(reload_config))
        .fallback(proxy_handler)
        .with_state(config);

    info!("Starting proxy server on 0.0.0.0:3000");

    // Run it
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> Response {
    Response::builder()
        .status(200)
        .body(Body::from("OK"))
        .unwrap()
}

async fn get_config(State(config): State<Arc<ConfigHolder>>) -> impl IntoResponse {
    let config = config.get();
    let config_json = serde_json::to_string_pretty(&*config).unwrap();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(config_json))
        .unwrap()
}

async fn reload_config(State(config): State<Arc<ConfigHolder>>) -> impl IntoResponse {
    match config.reload() {
        Ok(()) => Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("Configuration reloaded successfully"))
            .unwrap(),
        Err(e) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("Failed to reload config: {}", e)))
            .unwrap(),
    }
}

#[axum::debug_handler]
async fn proxy_handler(State(config): State<Arc<ConfigHolder>>, req: Request) -> Response {
    let start_time = std::time::Instant::now();

    // DO NOT ADD ANYTHING ABOVE THIS LINE - THIS PORTION IS EXTREMELY LATENCY SENSITIVE
    // Extract target URL from path
    let target_uri = req.uri().path().strip_prefix("/").unwrap_or("");
    if target_uri.is_empty() {
        return Response::builder()
            .status(400)
            .body(Body::from("Missing target URL in path - example: http://localhost:3000/https://httpbin.org/post"))
            .unwrap();
    }

    // Reconstruct full target URL including query parameters
    let target_url = if let Some(query) = req.uri().query() {
        format!("{}?{}", target_uri, query)
    } else {
        target_uri.to_string()
    };

    let method = req.method().clone();
    let headers = req.headers().clone();
    let uri = req.uri().clone();

    // Check if request should be dropped
    if let Some(drop_response) = config.get().should_drop_request(&req) {
        return Response::builder()
            .status(drop_response.status_code)
            .body(Body::from(drop_response.body.clone().unwrap_or_default()))
            .unwrap();
    }

    // Capture request body
    let request_body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .unwrap();
    let request_body = String::from_utf8(request_body_bytes.clone().to_vec())
        .map(|s| s.to_string())
        .unwrap_or_else(|e| format!("<invalid UTF-8: {}>", e));

    // Forward the request using reqwest
    let client = reqwest::Client::new();
    let endpoint_start_time = std::time::Instant::now();
    let resp = client
        .request(
            reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap(),
            target_url.clone(),
        )
        .headers(reqwest::header::HeaderMap::from_iter(headers.iter().map(
            |(k, v)| {
                (
                    reqwest::header::HeaderName::from_bytes(k.as_ref()).unwrap(),
                    reqwest::header::HeaderValue::from_bytes(v.as_bytes()).unwrap(),
                )
            },
        )))
        .body(reqwest::Body::from(request_body_bytes))
        .send()
        .await;
    let endpoint_duration_us = endpoint_start_time.elapsed().as_micros();

    let (status, response_body, response) = match resp {
        Ok(resp) => {
            let status = resp.status();
            let body_bytes = resp.bytes().await.unwrap();
            (
                status,
                String::from_utf8(body_bytes.clone().to_vec())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|e| format!("<invalid UTF-8: {}>", e)),
                Response::builder()
                    .status(status.as_u16())
                    .body(Body::from(body_bytes))
                    .unwrap(),
            )
        }
        Err(e) => {
            let error_msg = format!("Bad Gateway: {}", e);
            let response = Response::builder()
                .status(502)
                .body(Body::from(error_msg.clone()))
                .unwrap();
            (reqwest::StatusCode::BAD_GATEWAY, error_msg, response)
        }
    };
    let total_duration_us = start_time.elapsed().as_micros();

    // ONLY ADD ADDITIONAL CODE INSIDE THE ASYNC BLOCK - ANYTHING ABOVE THIS LINE IS EXTREMELY LATENCY SENSITIVE
    tokio::spawn({
        let config = config.clone();
        let headers = headers.clone();
        async move {
            let should_log = {
                let config = config.get();
                let req = Request::builder()
                    .method(method.clone())
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap();
                config.should_log_request(&req).cloned()
            };

            // Only log if the request matches our rules
            if let Some(_capture_config) = should_log {
                info!(
                    request.method = %method,
                    request.uri = %target_url,
                    request.body = %request_body,
                    request.headers = ?headers,
                    response.status = %status,
                    response.body = %response_body,
                    response.endpoint_duration_ms = endpoint_duration_us as f64 / 1000.0,
                    response.total_duration_ms = total_duration_us as f64 / 1000.0,
                    response.overhead_duration_ms = (total_duration_us - endpoint_duration_us) as f64 / 1000.0,
                    "Request completed"
                );
            }
        }
    });

    response
}
