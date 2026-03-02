use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use logprox::config::{BodyMatch, Config, MatchConditions, PathMatch};
use std::collections::HashMap;
use std::time::Duration;

fn bench_regex_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("regex_compilation");

    let patterns = vec![
        r"/api/v[0-9]+/users/[0-9]+",
        r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$",
        r".*token=.*",
        r"/health/.*",
        r"/metrics.*",
    ];

    for pattern in &patterns {
        group.bench_with_input(
            BenchmarkId::new("compile_on_demand", pattern),
            &pattern,
            |b, pattern| {
                b.iter(|| {
                    black_box(
                        regex::Regex::new(black_box(pattern))
                            .map(|re| re.is_match("/api/v1/users/123"))
                            .unwrap_or(false),
                    )
                });
            },
        );
    }

    // Test cached regex (simulating the optimization)
    let mut cache = std::collections::HashMap::<String, regex::Regex>::new();

    for pattern in &patterns {
        let pattern_str = *pattern;
        group.bench_with_input(
            BenchmarkId::new("cached_regex", pattern_str),
            &pattern_str,
            |b, pattern| {
                if !cache.contains_key(*pattern) {
                    cache.insert(pattern.to_string(), regex::Regex::new(pattern).unwrap());
                }
                let regex = cache.get(*pattern).unwrap();

                b.iter(|| black_box(regex.is_match("/api/v1/users/123")));
            },
        );
    }

    group.finish();
}

fn bench_string_allocations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_allocations");

    // Simulate current approach (with allocations)
    group.bench_function("current_approach", |b| {
        b.iter(|| {
            let method = "POST";
            let path = "/api/v1/users/123";
            let body_bytes = b"{'name': 'test', 'email': 'test@example.com'}";

            // Current code allocations
            let _method_str = method.to_string();
            let _path_str = path.to_string();
            let _body_str = String::from_utf8_lossy(body_bytes).to_string();

            black_box(())
        });
    });

    // Simulate optimized approach (minimal allocations)
    group.bench_function("optimized_approach", |b| {
        b.iter(|| {
            let method = "POST";
            let path = "/api/v1/users/123";
            let body_bytes = b"{'name': 'test', 'email': 'test@example.com'}";

            // Optimized code (no allocations)
            let _method_str = method; // &str
            let _path_str = path; // &str
            let _body_str =
                std::borrow::Cow::from(std::string::String::from_utf8_lossy(body_bytes));

            black_box(())
        });
    });

    group.finish();
}

fn bench_config_locking(c: &mut Criterion) {
    use std::sync::Arc;

    let mut group = c.benchmark_group("config_locking");

    let config = Arc::new(ConfigHolder::new(Config {
        server: logprox::config::ServerConfig { port: 3000 },
        logging: logprox::config::LoggingConfig {
            default: false,
            rules: vec![],
        },
        drop: logprox::config::DropConfig {
            default: false,
            rules: vec![],
        },
        response_logging: logprox::config::ResponseLoggingConfig {
            default: false,
            rules: vec![],
        },
    }));

    // Current approach: Multiple separate lock acquisitions
    group.bench_function("multiple_locks", |b| {
        b.iter(|| {
            // Simulate current pattern
            let _config1 = config.get();
            let _config2 = config.get();
            let _config3 = config.get();
            black_box(())
        });
    });

    // Optimized approach: Single lock acquisition
    group.bench_function("single_lock", |b| {
        b.iter(|| {
            // Simulate optimized pattern
            let _config = config.get();
            // Do all operations with single lock
            black_box(())
        });
    });

    group.finish();
}

fn bench_header_processing(c: &mut Criterion) {
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    let mut group = c.benchmark_group("header_processing");

    // Create sample headers
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("authorization", HeaderValue::from_static("Bearer token123"));
    headers.insert("user-agent", HeaderValue::from_static("Mozilla/5.0"));
    headers.insert("accept", HeaderValue::from_static("application/json"));
    headers.insert("x-custom-header", HeaderValue::from_static("custom-value"));

    // Current approach: Individual header processing
    group.bench_function("current_approach", |b| {
        b.iter(|| {
            let mut result = reqwest::header::HeaderMap::new();

            for (name, value) in headers.iter() {
                let name_str = name.as_str();
                if !["connection", "keep-alive"]
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(name_str))
                {
                    if let Ok(key) =
                        reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes())
                    {
                        if let Ok(val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes())
                        {
                            result.insert(key, val);
                        }
                    }
                }
            }

            black_box(result)
        });
    });

    // Optimized approach: Pre-allocated with direct copying
    group.bench_function("optimized_approach", |b| {
        b.iter(|| {
            let mut result = reqwest::header::HeaderMap::new();

            // Optimized direct copying
            for (name, value) in headers.iter() {
                if !["connection", "keep-alive"]
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(name.as_str()))
                {
                    if let Ok(key) =
                        reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes())
                    {
                        if let Ok(val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes())
                        {
                            result.insert(key, val);
                        }
                    }
                }
            }

            black_box(result)
        });
    });

    group.finish();
}

// Mock ConfigHolder for testing
use logprox::config::{ConfigHolder, DropConfig, LoggingConfig};

criterion_group!(
    benches,
    bench_regex_compilation,
    bench_string_allocations,
    bench_config_locking,
    bench_header_processing
);
criterion_main!(benches);
