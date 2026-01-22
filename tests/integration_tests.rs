use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use logprox::config::{Config, ConfigHolder, ServerConfig, LoggingConfig, DropConfig, DropRule, MatchConditions, PathMatch, BodyMatch, DropResponse, ResponseLoggingConfig, LoggingRule, CaptureConfig};
use logprox::{get_config, get_config_docs, get_health_check, proxy_handler, reload_config};
use std::collections::HashMap;
use std::sync::Arc;
use tower::util::ServiceExt;

fn load_test_config() -> Config {
    Config::from_file("tests/test_config.yaml").unwrap()
}

fn create_test_app(config: Config) -> Router {
    let config = Arc::new(ConfigHolder::new(config));
    Router::new()
        .route("/health", axum::routing::get(get_health_check))
        .route("/config", axum::routing::get(get_config))
        .route("/config/docs", axum::routing::get(get_config_docs))
        .route("/config/reload", axum::routing::post(reload_config))
        .fallback(proxy_handler)
        .with_state(config)
}

#[tokio::test]
async fn test_health_check() {
    let config = Arc::new(ConfigHolder::new(Config {
        server: ServerConfig { port: 3000 },
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
        server: ServerConfig { port: 3000 },
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
async fn test_get_config_docs() {
    let config = Arc::new(ConfigHolder::new(Config {
        server: ServerConfig { port: 3000 },
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

    assert!(body_str.contains("# LogProx Configuration Documentation"));
    assert!(body_str.contains("## Configuration Structure"));
    assert!(body_str.contains("## API Endpoints"));
    assert!(body_str.contains("GET /config/docs"));
}

#[tokio::test]
async fn test_proxy_handler_drop_request() {
    let config = Arc::new(ConfigHolder::new(Config {
        server: ServerConfig { port: 3000 },
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
async fn test_forward_get_request() {
    let config = load_test_config();
    let app = create_test_app(config);

    let req = Request::builder()
        .method("GET")
        .uri("/https://httpbin.org/get")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["url"], "https://httpbin.org/get");
}

#[tokio::test]
async fn test_forward_post_request() {
    let config = load_test_config();
    let app = create_test_app(config);

    let test_body = r#"{"test": "data"}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/https://httpbin.org/post")
        .header("content-type", "application/json")
        .body(Body::from(test_body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["json"]["test"], "data");
}

#[tokio::test]
async fn test_forward_various_methods() {
    let config = load_test_config();
    let app = create_test_app(config);

    let methods = [("GET", "get"), ("PUT", "put"), ("DELETE", "delete")];
    for (method, path) in methods {
        let req = Request::builder()
            .method(method)
            .uri(&format!("/https://httpbin.org/{}", path))
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn test_header_forwarding() {
    let config = load_test_config();
    let app = create_test_app(config);

    let req = Request::builder()
        .method("GET")
        .uri("/https://httpbin.org/headers")
        .header("x-custom-header", "test-value")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["headers"]["X-Custom-Header"], "test-value");
}

#[tokio::test]
async fn test_forward_various_status_codes() {
    let config = load_test_config();
    let app = create_test_app(config);

    let status_tests = [200, 404, 500, 503];
    for status in status_tests {
        let req = Request::builder()
            .method("GET")
            .uri(&format!("/https://httpbin.org/status/{}", status))
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status().as_u16(), status);
    }
}

#[tokio::test]
async fn test_timeout_with_short_timeout() {
    let config = Config {
        server: ServerConfig { port: 3000 },
        logging: LoggingConfig {
            default: false,
            rules: vec![LoggingRule {
                name: "Short timeout".into(),
                match_conditions: MatchConditions {
                    path: PathMatch { patterns: vec!["httpbin.org/delay.*".to_string()] },
                    methods: vec![],
                    headers: HashMap::new(),
                    body: BodyMatch { patterns: vec![] },
                },
                capture: CaptureConfig {
                    headers: vec![],
                    body: false,
                    method: true,
                    path: true,
                    timing: true,
                },
                timeout: Some("2s".to_string()),
            }],
        },
        drop: DropConfig { default: false, rules: vec![] },
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    };

    let app = create_test_app(config);

    let req = Request::builder()
        .method("GET")
        .uri("/https://httpbin.org/delay/10")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Upstream timeout");
}

#[tokio::test]
async fn test_timeout_with_no_timeout_rule() {
    let config = Config {
        server: ServerConfig { port: 3000 },
        logging: LoggingConfig {
            default: false,
            rules: vec![LoggingRule {
                name: "No timeout".into(),
                match_conditions: MatchConditions {
                    path: PathMatch { patterns: vec!["httpbin.org/.*".to_string()] },
                    methods: vec![],
                    headers: HashMap::new(),
                    body: BodyMatch { patterns: vec![] },
                },
                capture: CaptureConfig {
                    headers: vec![],
                    body: false,
                    method: true,
                    path: true,
                    timing: true,
                },
                timeout: None,
            }],
        },
        drop: DropConfig { default: false, rules: vec![] },
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    };

    let app = create_test_app(config);

    let req = Request::builder()
        .method("GET")
        .uri("/https://httpbin.org/delay/1")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_upstream_error_handling() {
    let config = Config {
        server: ServerConfig { port: 3000 },
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig { default: false, rules: vec![] },
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    };

    let app = create_test_app(config);

    let req = Request::builder()
        .method("GET")
        .uri("/https://this-domain-does-not-exist-12345.com/")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn test_malformed_upstream_url() {
    let config = Config {
        server: ServerConfig { port: 3000 },
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig { default: false, rules: vec![] },
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    };

    let app = create_test_app(config);

    let req = Request::builder()
        .method("GET")
        .uri("/not-a-valid-url")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_empty_upstream_url() {
    let config = Config {
        server: ServerConfig { port: 3000 },
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig { default: false, rules: vec![] },
        response_logging: ResponseLoggingConfig { default: false, rules: vec![] },
    };

    let app = create_test_app(config);

    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}