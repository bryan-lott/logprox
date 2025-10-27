use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use logprox::config::{Config, ConfigHolder, ServerConfig, LoggingConfig, DropConfig, DropRule, MatchConditions, PathMatch, BodyMatch, DropResponse, ResponseLoggingConfig};
use logprox::{get_config, get_config_docs, get_health_check, proxy_handler, reload_config};
use std::collections::HashMap;
use std::sync::Arc;
use tower::util::ServiceExt;

#[tokio::test]
async fn test_health_check() {
    let config = Arc::new(ConfigHolder::new(Config {
        server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig { default: false, rules: vec![] },
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    }));
    let app = Router::new()
        .route("/health", axum::routing::get(get_health_check))
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
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    }));
    let app = Router::new()
        .route("/config", axum::routing::get(get_config))
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
        server: ServerConfig { port: 3000, config_file: "nonexistent.yaml".to_string() },
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig { default: false, rules: vec![] },
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    }));
    let app = Router::new()
        .route("/config/reload", axum::routing::post(reload_config))
        .with_state(config);

    let req = Request::builder()
        .method("POST")
        .uri("/config/reload")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Since the file doesn't exist, it should return 500
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_get_config_docs() {
    let config = Arc::new(ConfigHolder::new(Config {
        server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig { default: false, rules: vec![] },
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    }));

    let app = Router::new()
        .route("/config/docs", axum::routing::get(get_config_docs))
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
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
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
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
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

    // Should complete in under 10ms (exceptionally fast for drop requests)
    assert!(duration < std::time::Duration::from_millis(10),
            "Drop request took too long: {:?}", duration);

    // Should get the expected response
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_proxy_no_significant_overhead() {
    // Test that the proxy handler processes drop requests quickly
    // We test the decision-making logic without actual network calls

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
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
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

        // Drop requests should complete in under 10ms (exceptionally fast)
        assert!(proxy_duration < std::time::Duration::from_millis(10),
                "Drop request took too long on iteration {}: {:?}", i, proxy_duration);

        // Verify it was actually dropped
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}