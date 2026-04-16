use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_regex_optimization(c: &mut Criterion) {
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

    group.bench_function("uncached_compilation", |b| {
        b.iter(|| {
            for (pattern, test_str) in patterns.iter().zip(test_strings.iter()) {
                let regex = regex::Regex::new(black_box(pattern)).unwrap();
                black_box(regex.is_match(black_box(test_str)));
            }
        });
    });

    group.finish();
}

fn bench_header_optimization(c: &mut Criterion) {
    use axum::http::{HeaderMap, HeaderValue};

    let complex_headers = {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("authorization", HeaderValue::from_static("Bearer token123"));
        headers.insert("x-request-id", HeaderValue::from_static("req-123-456"));
        headers.insert("x-user-agent", HeaderValue::from_static("test-agent/1.0"));
        headers.insert("x-forwarded-for", HeaderValue::from_static("192.168.1.1"));
        headers.insert("connection", HeaderValue::from_static("keep-alive"));
        headers
    };

    let mut group = c.benchmark_group("header_optimization");

    group.bench_function("header_iteration", |b| {
        b.iter(|| {
            let mut count = 0;
            for (name, _value) in complex_headers.iter() {
                if !["connection", "keep-alive"]
                    .iter()
                    .any(|h| h.eq_ignore_ascii_case(name.as_str()))
                {
                    count += 1;
                }
            }
            black_box(count);
        });
    });

    group.finish();
}

fn bench_memory_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_optimization");

    group.bench_function("vec_allocation", |b| {
        b.iter(|| {
            let buf = vec![0u8; 4096];
            black_box(buf);
        });
    });

    group.bench_function("vec_with_capacity", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(4096);
            buf.resize(4096, 0);
            black_box(buf);
        });
    });

    group.finish();
}

fn bench_yaml_parsing(c: &mut Criterion) {
    use logprox::config::Config;

    let yaml_content = r#"
logging:
  default: false
  rules:
    - name: "test_rule"
      match_conditions:
        path:
          patterns: ["/api/.*"]
        methods: ["GET", "POST"]
      capture:
        method: true
        path: true
        timing: true

drop:
  default: false
  rules: []
"#;

    let mut group = c.benchmark_group("yaml_parsing");

    group.bench_function("parse_config", |b| {
        b.iter(|| {
            let config: Config = serde_norway::from_str(black_box(yaml_content)).unwrap();
            black_box(config);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_regex_optimization,
    bench_header_optimization,
    bench_memory_optimization,
    bench_yaml_parsing
);
criterion_main!(benches);
