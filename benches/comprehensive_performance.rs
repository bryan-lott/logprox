use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use reqwest::Client;
use axum::{Router, routing::fallback, routing::get, extract::Request};
use logprox::{proxy_handler, performance::{RegexCache, HeaderProcessor, BytesPool, LockFreeConfigHolder}, config::{Config, ConfigHolder, ServerConfig, LoggingConfig, DropConfig}};

async fn setup_performance_test_servers() -> (String, String) {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    tokio::spawn(async move {
        let upstream_app = Router::new()
            .route("/api/v1/users", get(|| async { 
                axum::Json(serde_json::json!({"users": [{"id": 1, "name": "test"}]})) 
            }))
            .route("/health", get(|| async { axum::Json(serde_json::json!({"status": "ok"})) }))
            .route("/api/v1/users/123", get(|| async { 
                axum::Json(serde_json::json!({"id": 123, "name": "John Doe"})) 
            }));

        axum::serve(upstream_listener, upstream_app).await.unwrap();
    });

    let config = Config {
        server: ServerConfig { port: 0 },
        logging: LoggingConfig { 
            default: false, 
            rules: vec![
                logprox::config::LoggingRule {
                    name: "api_logging".to_string(),
                    match_conditions: logprox::config::MatchConditions {
                        path: logprox::config::PathMatch {
                            patterns: vec![r"/api/v[0-9]+/.*".to_string()],
                        },
                        methods: vec!["GET".to_string(), "POST".to_string()],
                        headers: std::collections::HashMap::new(),
                        body: logprox::config::BodyMatch { patterns: vec![] },
                    },
                    capture: logprox::config::CaptureConfig {
                        headers: vec![],
                        body: true,
                        method: true,
                        path: true,
                        timing: true,
                    },
                    timeout: Some("5000ms".to_string()),
                }
            ] 
        },
        drop: DropConfig { default: false, rules: vec![] },
    };
    let config_holder = Arc::new(ConfigHolder::new(config));

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    tokio::spawn(async move {
        let proxy_app = Router::new()
            .fallback(proxy_handler)
            .with_state(config_holder);

        axum::serve(proxy_listener, proxy_app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    (
        format!("http://{}", upstream_addr),
        format!("http://{}", proxy_addr),
    )
}

fn bench_proxy_with_complex_config(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let (upstream_url, proxy_url) = setup_performance_test_servers().await;
        let client = Client::new();

        // Warm up
        client.get(format!("{}/health", upstream_url)).send().await.ok();
        
        // Direct request baseline
        c.bench_function("direct_request_with_body", |b| {
            b.to_async(&rt).iter(|| async {
                let response = client
                    .get(format!("{}/api/v1/users/123", upstream_url))
                    .send()
                    .await;
                black_box(response).ok();
            });
        });

        // Proxy with complex regex matching
        c.bench_function("proxy_with_regex_matching", |b| {
            b.to_async(&rt).iter(|| async {
                let response = client
                    .get(format!("{}/{}", proxy_url, upstream_url.trim_start_matches("http://")))
                    .header("User-Agent", "Test-Agent/1.0")
                    .send()
                    .await;
                black_box(response).ok();
            });
        });

        // Proxy with POST request (body processing)
        c.bench_function("proxy_with_body_processing", |b| {
            b.to_async(&rt).iter(|| async {
                let response = client
                    .post(format!("{}/api/v1/users", upstream_url))
                    .json(&serde_json::json!({
                        "name": "Test User",
                        "email": "test@example.com"
                    }))
                    .send()
                    .await;
                black_box(response).ok();
            });
        });
    });
}

fn bench_proxy_overhead_components(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let (upstream_url, proxy_url) = setup_performance_test_servers().await;
        let client = Client::new();

        // Measure just the routing overhead (no upstream call)
        c.bench_function("proxy_routing_only", |b| {
            b.to_async(&rt).iter(|| async {
                // This will test just the routing and config checking
                let response = client
                    .get(format!("{}/{}", proxy_url, "invalid-url"))
                    .send()
                    .await;
                black_box(response).ok();
            });
        });

        // Measure header processing overhead
        c.bench_function("proxy_with_many_headers", |b| {
            b.to_async(&rt).iter(|| async {
                let mut req = client
                    .get(format!("{}/{}", proxy_url, upstream_url.trim_start_matches("http://")));
                
                // Add many headers to test header processing overhead
                for i in 0..20 {
                    req = req.header(&format!("X-Custom-Header-{}", i), &format!("value-{}", i));
                }
                
                let response = req.send().await;
                black_box(response).ok();
            });
        });
    });
}

// Add new performance benchmarks
fn bench_regex_optimization(c: &mut Criterion) {
    let cache = RegexCache::new();
    let patterns = vec![
        r"\d+",
        r"/api/v[0-9]+/.*",
        r"user_[a-zA-Z0-9]+",
        r"Bearer .*",
        r"application/json",
    ];
    let test_strings = vec![
        "12345",
        "/api/v1/users/123",
        "user_admin123",
        "Bearer token123",
        "application/json",
    ];

    let mut group = c.benchmark_group("regex_optimization");

    // Benchmark uncached vs cached
    group.bench_function("uncached_compilation", |b| {
        b.iter(|| {
            for (pattern, test_str) in patterns.iter().zip(test_strings.iter()) {
                let regex = regex::Regex::new(black_box(pattern)).unwrap();
                black_box(regex.is_match(black_box(test_str)));
            }
        });
    });

    group.bench_function("cached_access", |b| {
        // Warm up cache
        for pattern in &patterns {
            cache.get_or_compile(pattern).unwrap();
        }

        b.iter(|| {
            for (pattern, test_str) in patterns.iter().zip(test_strings.iter()) {
                let regex = cache.get_or_compile(black_box(pattern)).unwrap();
                black_box(regex.is_match(black_box(test_str)));
            }
        });
    });

    group.finish();
}

fn bench_header_optimization(c: &mut Criterion) {
    let processor = HeaderProcessor::new();
    
    let complex_headers = {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("content-type", "application/json");
        headers.insert("authorization", "Bearer token123");
        headers.insert("x-request-id", "req-123-456");
        headers.insert("x-user-agent", "test-agent/1.0");
        headers.insert("x-forwarded-for", "192.168.1.1");
        headers.insert("connection", "keep-alive");
        headers
    };

    let mut group = c.benchmark_group("header_optimization");

    group.bench_function("optimized_filtering", |b| {
        b.iter(|| {
            let filtered = processor.filter_headers_reqwest(black_box(&complex_headers));
            black_box(filtered);
        });
    });

    group.finish();
}

fn bench_memory_optimization(c: &mut Criterion) {
    let pool = BytesPool::new();
    
    let mut group = c.benchmark_group("memory_optimization");

    group.bench_function("pool_vs_allocation", |b| {
        b.iter(|| {
            let buf = pool.get_buffer(4096);
            black_box(buf);
        });
    });

    group.finish();
}

criterion_group!(
    benches, 
    bench_proxy_with_complex_config, 
    bench_proxy_overhead_components,
    bench_regex_optimization,
    bench_header_optimization,
    bench_memory_optimization
);
criterion_main!(benches);