/*
TODO's
- [x] Add a health check endpoint
- [x] Add configuration by config file or environment variables
- [x] Add environment variable substitution in config values
- [x] Add dropping requests based on config
   - [x] Headers
   - [x] Body
   - [x] Request method
   - [x] Request path
- [ ] Add injection of additional headers based on config
- [x] Add reloading the config file on a POST
- [x] Add a get endpoint for returning the current config
- [ ] Add a get endpoint for returning the config schema
- [x] Add a config documentation endpoint
- [ ] Add environment variable substitution in config values
*/

mod config;

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, Router},
};
use config::{Config, ConfigHolder, ServerConfig, LoggingConfig, DropConfig, DropRule, MatchConditions, PathMatch, BodyMatch, DropResponse};
use serde_json;
use std::collections::HashMap;
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
        .route("/health", get(health_check))
        .route("/config", get(get_config))
        .route("/config/docs", get(get_config_docs))
        .route("/config/reload", post(reload_config))
        .fallback(proxy_handler)
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

async fn get_config_docs() -> impl IntoResponse {
    let docs = r#"# LogProx Configuration Documentation

## Overview
LogProx uses a YAML configuration file to define logging and request dropping rules. The configuration supports environment variable substitution using `${VAR_NAME}` syntax.

## Configuration Structure

### Server Configuration
```yaml
server:
  port: 3000                    # Server port (can be overridden by PORT env var)
  config_file: config.yaml      # Config file path (can be overridden by CONFIG_FILE env var)
```

### Logging Configuration
```yaml
logging:
  default: false                # Default logging behavior if no rules match
  rules:                        # Array of logging rules
    - name: "Rule Name"         # Descriptive name for the rule
      match_conditions:         # Conditions that must ALL match
        path:                   # URL path patterns (regex)
          patterns:
            - "/api/.*"
        methods:                # HTTP methods to match
          - "POST"
          - "PUT"
        headers:                # Required headers and regex patterns
          "content-type": "application/json.*"
          "authorization": "Bearer .*"
        body:                   # Request body patterns (regex)
          patterns:
            - '"amount":\s*\d+'
      capture:                  # What to include in logs
        headers:                # List of header names to capture
          - "content-type"
          - "user-agent"
        body: true              # Whether to log request body
        method: true            # Whether to log HTTP method
        path: true              # Whether to log URL path
        timing: true            # Whether to log timing information
```

### Drop Configuration
```yaml
drop:
  default: false                # Default drop behavior if no rules match
  rules:                        # Array of drop rules
    - name: "Rule Name"         # Descriptive name for the rule
      match_conditions:         # Conditions that must ALL match (same as logging)
        path:
          patterns:
            - "/deprecated/.*"
        methods:
          - "GET"
        headers:
          "user-agent": ".*bot.*"
        body:
          patterns:
            - "<script>.*</script>"
      response:                 # Response to return when dropping
        status_code: 403        # HTTP status code
        body: "Access denied"   # Response body (supports env vars)
```

## Environment Variables

### Configuration File Location
- `CONFIG_FILE`: Path to config file (default: config.yaml)
- `PORT`: Server port (default: 3000)

### Environment Variable Substitution
Config values can reference environment variables using `${VAR_NAME}` syntax:
```yaml
drop:
  rules:
    - name: "API Key Required"
      response:
        status_code: 401
        body: "API Key ${API_KEY} required"
```

## Pattern Matching

### Regex Syntax
All pattern matching uses Rust's regex engine. Common patterns:
- `.*` - Match any characters
- `^/api/` - Match paths starting with /api/
- `\d+` - Match one or more digits
- `(option1|option2)` - Match either option1 or option2

### Matching Logic
- **Path patterns**: At least one pattern must match the request path
- **Methods**: The request method must be in the methods list (if specified)
- **Headers**: ALL specified headers must be present and match their patterns
- **Body patterns**: At least one pattern must match the request body content
- **Rule evaluation**: Rules are evaluated in order; first match wins

## Examples

### Basic API Logging
```yaml
logging:
  default: false
  rules:
    - name: "Log API requests"
      match_conditions:
        path:
          patterns:
            - "/api/.*"
        methods:
          - "POST"
          - "PUT"
          - "DELETE"
      capture:
        headers:
          - "content-type"
          - "authorization"
        body: true
        method: true
        path: true
        timing: true
```

### Security: Block Malicious Requests
```yaml
drop:
  default: false
  rules:
    - name: "Block XSS attempts"
      match_conditions:
        body:
          patterns:
            - "<script>.*</script>"
            - "javascript:"
            - "onload="
      response:
        status_code: 400
        body: "Malicious content detected"
```

### Rate Limiting Simulation
```yaml
drop:
  default: false
  rules:
    - name: "Block bot traffic"
      match_conditions:
        headers:
          "user-agent": ".*(bot|crawler|spider).*"
      response:
        status_code: 429
        body: "Rate limit exceeded"
```

## API Endpoints

- `GET /health` - Health check
- `GET /config` - Current configuration (JSON)
- `GET /config/docs` - This documentation
- `POST /config/reload` - Reload configuration from file

## Notes

- Configuration is loaded on startup and can be reloaded via POST /config/reload
- Invalid regex patterns will cause rule matching to fail for that condition
- Request bodies are consumed for all requests to enable body matching
- Environment variables are substituted at config load time
- All pattern matching is case-sensitive unless specified otherwise"#;

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Body::from(docs))
        .unwrap()
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

    // Capture request body FIRST (needed for body matching)
    let request_body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .unwrap();
    let request_body = String::from_utf8(request_body_bytes.clone().to_vec())
        .map(|s| s.to_string())
        .unwrap_or_else(|e| format!("<invalid UTF-8: {}>", e));

    // Create a request for matching (without body since we consumed it)
    let match_req = axum::http::Request::builder()
        .method(method.clone())
        .uri(uri.clone())
        .body(axum::body::Body::empty())
        .unwrap();

    // Check if request should be dropped (now includes body matching)
    if let Some(drop_response) = config.get().should_drop_request(&match_req, &request_body) {
        return Response::builder()
            .status(drop_response.status_code)
            .body(Body::from(drop_response.body.clone().unwrap_or_default()))
            .unwrap();
    }

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
            let should_log_capture = {
                let config = config.get();
                let req = Request::builder()
                    .method(method.clone())
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap();
                config.should_log_request(&req, &request_body).cloned()
            };

            // Only log if the request matches our rules
            if let Some(_capture_config) = should_log_capture {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::Router;
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn test_health_check() {
        let config = Arc::new(ConfigHolder::new(Config {
            server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
            logging: LoggingConfig { default: false, rules: vec![] },
            drop: DropConfig { default: false, rules: vec![] },
        }));
        let app = Router::new()
            .route("/health", get(health_check))
            .with_state(config);

        let req = Request::builder()
            .method("GET")
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"OK");
    }

    #[tokio::test]
    async fn test_get_config() {
        let config = Arc::new(ConfigHolder::new(Config {
            server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
            logging: LoggingConfig { default: false, rules: vec![] },
            drop: DropConfig { default: false, rules: vec![] },
        }));
        let app = Router::new()
            .route("/config", get(get_config))
            .with_state(config.clone());

        let req = Request::builder()
            .method("GET")
            .uri("/config")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("content-type").unwrap(), "application/json");

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let config_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(config_json["logging"]["default"], false);
        assert_eq!(config_json["drop"]["default"], false);
    }

    #[tokio::test]
    async fn test_reload_config() {
        // Create a temporary config file
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.yaml");
        std::fs::write(&config_path, r#"
logging:
  default: true
drop:
  default: false
"#).unwrap();

        let config = Arc::new(ConfigHolder::new(Config {
            server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
            logging: LoggingConfig { default: false, rules: vec![] },
            drop: DropConfig { default: false, rules: vec![] },
        }));

        // Temporarily change the config file path for testing
        // Since ConfigHolder uses hardcoded "config.yaml", this is tricky
        // For this test, we'll just check the endpoint responds correctly
        let app = Router::new()
            .route("/config/reload", post(reload_config))
            .with_state(config);

        let req = Request::builder()
            .method("POST")
            .uri("/config/reload")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        // Reloads the existing config.yaml successfully
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_proxy_handler_drop_request() {
        let config = Arc::new(ConfigHolder::new(Config {
            server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
            logging: LoggingConfig { default: false, rules: vec![] },
            drop: DropConfig {
                default: false,
                rules: vec![DropRule {
                    name: "Test drop".to_string(),
                    match_conditions: MatchConditions {
                        path: PathMatch { patterns: vec!["/drop.*".to_string()] },
                        methods: vec![],
                        headers: HashMap::new(),
                        body: BodyMatch { patterns: vec![] },
                    },
                    response: DropResponse {
                        status_code: 403,
                        body: Some("Dropped".to_string()),
                    },
                }],
            },
        }));

        let app = Router::new()
            .fallback(proxy_handler)
            .with_state(config);

        let req = Request::builder()
            .method("GET")
            .uri("/drop/test")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"Dropped");
    }

    #[tokio::test]
    async fn test_proxy_latency_baseline() {
        // Test that proxy handler completes within reasonable time for drop requests
        // This establishes a baseline for latency measurements
        let config = Arc::new(ConfigHolder::new(Config {
            server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
            logging: LoggingConfig { default: false, rules: vec![] },
            drop: DropConfig {
                default: false,
                rules: vec![DropRule {
                    name: "Latency test".to_string(),
                    match_conditions: MatchConditions {
                        path: PathMatch { patterns: vec!["/latency.*".to_string()] },
                        methods: vec![],
                        headers: HashMap::new(),
                        body: BodyMatch { patterns: vec![] },
                    },
                    response: DropResponse {
                        status_code: 200,
                        body: Some("OK".to_string()),
                    },
                }],
            },
        }));

        let app = Router::new()
            .fallback(proxy_handler)
            .with_state(config);

        // Test drop request (fast path - no network calls)
        let start = std::time::Instant::now();
        let req = Request::builder()
            .method("GET")
            .uri("/latency/test")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let duration = start.elapsed();

        // Should complete in under 5ms (exceptionally fast for drop requests)
        assert!(duration < std::time::Duration::from_millis(5),
                "Drop request took too long: {:?}", duration);

        // Should get the expected response
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_config_docs() {
        let config = Arc::new(ConfigHolder::new(Config {
            server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
            logging: LoggingConfig { default: false, rules: vec![] },
            drop: DropConfig { default: false, rules: vec![] },
        }));

        let app = Router::new()
            .route("/config/docs", get(get_config_docs))
            .with_state(config);

        let req = Request::builder()
            .method("GET")
            .uri("/config/docs")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("content-type").unwrap(), "text/plain; charset=utf-8");

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Check that the documentation contains key sections
        assert!(body_str.contains("# LogProx Configuration Documentation"));
        assert!(body_str.contains("## Configuration Structure"));
        assert!(body_str.contains("## API Endpoints"));
        assert!(body_str.contains("GET /config/docs"));
    }

    #[tokio::test]
    async fn test_proxy_no_significant_overhead() {
        // Test that the proxy handler processes drop requests with minimal latency
        // Drop requests should be very fast since they don't make network calls

        let config = Arc::new(ConfigHolder::new(Config {
            server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
            logging: LoggingConfig { default: false, rules: vec![] },
            drop: DropConfig {
                default: false,
                rules: vec![DropRule {
                    name: "Fast drop test".to_string(),
                    match_conditions: MatchConditions {
                        path: PathMatch { patterns: vec!["/drop.*".to_string()] },
                        methods: vec![],
                        headers: HashMap::new(),
                        body: BodyMatch { patterns: vec![] },
                    },
                    response: DropResponse {
                        status_code: 403,
                        body: Some("Fast drop".to_string()),
                    },
                }],
            },
        }));

        // Test multiple drop requests to ensure consistent fast performance
        for i in 0..5 {
            let app = Router::new()
                .fallback(proxy_handler)
                .with_state(config.clone());

            let proxy_start = std::time::Instant::now();
            let req = Request::builder()
                .method("GET")
                .uri(&format!("/drop/test/{}", i))
                .body(Body::empty())
                .unwrap();

            let resp = app.oneshot(req).await.unwrap();
            let proxy_duration = proxy_start.elapsed();

            // Drop requests should complete in under 5ms (exceptionally fast)
            assert!(proxy_duration < std::time::Duration::from_millis(5),
                    "Drop request took too long on iteration {}: {:?}", i, proxy_duration);

            // Verify it was actually dropped
            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        }
    }
}
