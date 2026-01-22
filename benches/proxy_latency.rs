use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use reqwest::Client;
use axum::{Router, routing::fallback, routing::get};
use logprox::{proxy_handler, config::{Config, ConfigHolder, ServerConfig, LoggingConfig, DropConfig}};

async fn setup_test_servers() -> (String, String) {
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    tokio::spawn(async move {
        let upstream_app = Router::new()
            .route("/test", get(|| async { "OK" }))
            .route("/status/200", get(|| async { axum::http::StatusCode::OK }));

        axum::serve(upstream_listener, upstream_app).await.unwrap();
    });

    let config = Config {
        server: ServerConfig { port: 0 },
        logging: LoggingConfig { default: false, rules: vec![] },
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

fn bench_proxy_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let (upstream_url, proxy_url) = setup_test_servers().await;
        let client = Client::new();

        client.get(format!("{}/test", upstream_url)).send().await.ok();

        c.bench_function("direct_request_latency", |b| {
            b.to_async(&rt).iter(|| async {
                let response = client
                    .get(format!("{}/test", upstream_url))
                    .send()
                    .await;
                black_box(response).ok();
            });
        });

        c.bench_function("proxy_request_latency", |b| {
            b.to_async(&rt).iter(|| async {
                let response = client
                    .get(format!("{}/{}", proxy_url, upstream_url.trim_start_matches("http://")))
                    .send()
                    .await;
                black_box(response).ok();
            });
        });
    });
}

criterion_group!(benches, bench_proxy_latency);
criterion_main!(benches);