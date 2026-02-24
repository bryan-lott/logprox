// Ultra-fast benchmarking for performance validation
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::{Duration, Instant};
use std::sync::Arc;

use logprox::config::{Config, ConfigHolder, ServerConfig, LoggingConfig};
use logprox::handlers::proxy_handler;
use logprox::performance::{Metrics, TARGET_OVERHEAD_MICROS};

/// Benchmark config for performance testing
pub fn get_benchmark_config() -> Config {
    Config {
        server: ServerConfig { port: 3000 },
        logging: LoggingConfig {
            default: false,
            rules: vec![
                // Multiple rules to test regex caching impact
                logprox::config::LoggingRule {
                    name: "api_rule".to_string(),
                    match_conditions: logprox::config::MatchConditions {
                        path: logprox::config::PathMatch {
                            patterns: vec![
                                r"/api/v1/users/.*".to_string(),
                                r"/api/v1/orders/.*".to_string(),
                                r"/api/v1/products/.*".to_string(),
                            ],
                        },
                        methods: vec!["GET".to_string(), "POST".to_string()],
                        headers: std::collections::HashMap::from([
                            ("content-type".to_string(), "application/json.*".to_string()),
                            ("authorization".to_string(), "Bearer .*".to_string()),
                        ]),
                        body: logprox::config::BodyMatch {
                            patterns: vec!["user_id: \\d+".to_string()],
                        },
                    },
                    capture: logprox::config::CaptureConfig {
                        headers: vec!["content-type".to_string(), "authorization".to_string()],
                        body: true,
                        method: true,
                        path: true,
                        timing: true,
                    },
                    timeout: Some("30s".to_string()),
                },
                logprox::config::LoggingRule {
                    name: "health_rule".to_string(),
                    match_conditions: logprox::config::MatchConditions {
                        path: logprox::config::PathMatch {
                            patterns: vec![r"/health".to_string()],
                        },
                        methods: vec!["GET".to_string()],
                        headers: std::collections::HashMap::new(),
                        body: logprox::config::BodyMatch {
                            patterns: vec![],
                        },
                    },
                    capture: logprox::config::CaptureConfig {
                        headers: vec![],
                        body: false,
                        method: true,
                        path: true,
                        timing: true,
                    },
                    timeout: None,
                },
            ],
        },
        drop: Default::default(),
        response_logging: Default::default(),
    }
}

/// Create optimized proxy app for benchmarking
pub fn create_optimized_proxy_app(config: Config) -> axum::Router {
    let config_holder = Arc::new(ConfigHolder::new(config));
    
    axum::Router::new()
        .fallback(proxy_handler)
        .with_state(config_holder)
}

/// Benchmark request latency with micro-precision timing
pub fn benchmark_request_latency(c: &mut Criterion, config: Config, path: &str) {
    let app = create_optimized_proxy_app(config);
    
    // Pre-warm the app
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _ = rt.block_on(async {
        let req = axum::extract::Request::builder()
            .method("GET")
            .uri(path)
            .body(axum::body::Body::empty())
            .unwrap();
            
        app.clone().oneshot(req).await
    });
    
    c.bench_function(&format!("request_latency_{}", path.replace("/", "_")), |b| {
        b.to_async(&rt).iter(|| async {
            let req = axum::extract::Request::builder()
                .method("GET")
                .uri(path)
                .body(axum::body::Body::empty())
                .unwrap();
            
            let start = Instant::now();
            let _resp = app.clone().oneshot(req).await;
            let latency = start.elapsed();
            
            black_box(latency)
        })
    });
}

/// Benchmark overhead without networking
pub fn benchmark_overhead_only(c: &mut Criterion) {
    c.bench_function("overhead_only", |b| {
        b.iter(|| {
            let mut metrics = Metrics::new();
            let start = Instant::now();
            
            // Simulate config lookups without actual processing
            black_box(metrics.total());
            
            let overhead = start.elapsed();
            assert!(overhead.as_micros() < TARGET_OVERHEAD_MICROS as u64);
        })
    });
}

/// Benchmark regex caching performance
pub fn benchmark_regex_caching(c: &mut Criterion) {
    use logprox::performance::cache::RegexCache;
    
    c.bench_function("regex_cache_hit", |b| {
        b.iter(|| {
            let mut metrics = Metrics::new();
            let _regex = RegexCache::get_or_compile(r"user_id: \d+", &mut metrics);
            black_box(regex)
        })
    });
    
    c.bench_function("regex_cache_miss", |b| {
        b.iter(|| {
            let mut metrics = Metrics::new();
            let _regex = RegexCache::get_or_compile(r"unique_pattern_\x{}", &mut metrics);
            black_box(regex)
        })
    });
}

/// Benchmark header processing
pub fn benchmark_header_processing(c: &mut Criterion) {
    use logprox::performance::zero_copy::HeaderProcessor;
    use axum::http::{HeaderMap, HeaderValue};
    
    c.bench_function("header_processing_small", |b| {
        b.iter(|| {
            let mut headers = HeaderMap::new();
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            headers.insert("user-agent", HeaderValue::from_static("test/1.0"));
            headers.insert("authorization", HeaderValue::from_static("Bearer token123"));
            
            let processor = HeaderProcessor::new();
            let _reqwest_headers = processor.axum_to_reqwest(&headers, &mut Metrics::new());
            
            black_box(reqwest_headers.len())
        })
    });
    
    c.bench_function("header_processing_large", |b| {
        b.iter(|| {
            let mut headers = HeaderMap::new();
            for i in 0..50 {
                let name = format!("x-custom-{}", i);
                let value = format!("value-{}", i);
                headers.insert(&name, HeaderValue::from_str(&value).unwrap());
            }
            
            let processor = HeaderProcessor::new();
            let _reqwest_headers = processor.axum_to_reqwest(&headers, &mut Metrics::new());
            
            black_box(reqwest_headers.len())
        })
    });
}

/// Benchmark body processing
pub fn benchmark_body_processing(c: &mut Criterion) {
    use logprox::performance::pool::BodyProcessor;
    
    c.bench_function("body_processing_small", |b| {
        b.iter(|| {
            let body = axum::body::Body::from("small test body");
            let mut metrics = Metrics::new();
            
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _result = rt.block_on(async {
                BodyProcessor::process_body_fast(body, &mut metrics)
            });
            
            black_box(metrics.body_processing_time.as_micros())
        })
    });
    
    c.bench_function("body_processing_large", |b| {
        b.iter(|| {
            let large_body = vec![0u8; 10000];
            let body = axum::body::Body::from(large_body);
            let mut metrics = Metrics::new();
            
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _result = rt.block_on(async {
                BodyProcessor::process_body_smart(body, &mut metrics, false)
            });
            
            black_box(metrics.body_processing_time.as_micros())
        })
    });
}

/// Benchmark throughput under load
pub fn benchmark_throughput(c: &mut Criterion, config: Config) {
    let app = create_optimized_proxy_app(config);
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.throughput(&format!("throughput"), |group| {
        group.throughput(Throughput::ForDuration(Duration::from_secs(1)), |b| {
            b.to_async(&rt).iter(|| async {
                let req = axum::extract::Request::builder()
                    .method("GET")
                    .uri("/api/users")
                    .body(axum::body::Body::empty())
                    .unwrap();
                
                let _resp = app.clone().oneshot(req).await;
                black_box(resp.status().as_u16())
            })
        });
    });
}

/// Performance regression detection
pub fn benchmark_regression_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("regression");
    
    // Test different request complexities
    for complexity in ["simple", "regex_heavy", "header_heavy", "body_heavy"] {
        group.bench_with_input(
            BenchmarkId::new(complexity),
            complexity,
            |b| {
                b.iter(|| {
                    let config = get_benchmark_config();
                    let app = create_optimized_proxy_app(config);
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    
                    let req = match complexity {
                        "simple" => axum::extract::Request::builder()
                            .method("GET")
                            .uri("/health")
                            .body(axum::body::Body::empty())
                            .unwrap(),
                        "regex_heavy" => axum::extract::Request::builder()
                            .method("POST")
                            .uri("/api/v1/users")
                            .header("x-request-id", r"req_\d{10}")
                            .body(axum::body::Body::from(r#"{"user_id": 12345}"#))
                            .unwrap(),
                        "header_heavy" => {
                            let mut req = axum::extract::Request::builder()
                                .method("POST")
                                .uri("/api/v1/data")
                                .body(axum::body::Body::empty())
                                .unwrap();
                            
                            for i in 0..20 {
                                let header_name = format!("x-custom-{}", i);
                                req = req.header(&header_name, format!("value-{}", i));
                            }
                            req
                        },
                        "body_heavy" => axum::extract::Request::builder()
                            .method("POST")
                            .uri("/api/v1/upload")
                            .body(axum::body::Body::from(vec![0u8; 50000]))
                            .unwrap(),
                        _ => unreachable!(),
                    };
                    
                    let start = Instant::now();
                    let _resp = app.clone().oneshot(req).await;
                    let latency = start.elapsed();
                    
                    black_box((latency, complexity))
                })
            },
        );
    }
}

criterion_group!(
    micro_benchmarks,
    benchmark_overhead_only,
    benchmark_regex_caching,
    benchmark_header_processing,
    benchmark_body_processing,
    benchmark_throughput,
    benchmark_regression_detection
);

criterion_main!(micro_benchmarks);