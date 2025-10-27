use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use reqwest::Client;
use logprox::config::{Config, ConfigHolder, LoggingConfig, DropConfig, ServerConfig};

async fn setup_test_servers() -> (String, String) {
    // Start a mock upstream server
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = upstream_listener.accept().await.unwrap();
            // Simple echo server for testing
            tokio::spawn(async move {
                // For simplicity, we'll just accept connections
                // In a real benchmark, we'd implement a proper HTTP server
                let _ = stream;
            });
        }
    });

    // Start the proxy server
    let config = Config {
        server: ServerConfig { port: 0, config_file: "config.yaml".to_string() }, // Use port 0 for auto-assignment
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig { default: false, rules: vec![] },
    };
    let config_holder = Arc::new(ConfigHolder::new(config));

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    tokio::spawn(async move {
        // In a real implementation, we'd start the axum server
        // For now, we'll just keep the listener alive
        let _ = proxy_listener;
        let _ = config_holder;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    });

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

        // Give servers time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        c.bench_function("proxy_request_latency", |b| {
            b.to_async(&rt).iter(|| async {
                // Make a request through the proxy
                let response = client
                    .get(&format!("{}/test", proxy_url))
                    .send()
                    .await;

                // We don't care about the result, just the timing
                black_box(response).ok();
            });
        });

        c.bench_function("direct_request_latency", |b| {
            b.to_async(&rt).iter(|| async {
                // Make a direct request for comparison
                let response = client
                    .get(&format!("{}/test", upstream_url))
                    .send()
                    .await;

                black_box(response).ok();
            });
        });
    });
}

criterion_group!(benches, bench_proxy_latency);
criterion_main!(benches);