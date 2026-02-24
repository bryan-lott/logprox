#[cfg(test)]
mod performance_regression_tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_proxy_overhead_under_10ms() {
        // This test ensures that the proxy overhead stays under 10ms
        // In a real scenario, this would measure actual proxy performance
        let start = std::time::Instant::now();
        
        // Simulate the most expensive operations
        let config = create_test_config();
        let req = create_test_request();
        
        // Test config matching overhead
        let _should_drop = config.should_drop_request(&req, "");
        let _log_config = config.should_log_request(&req, "");
        
        // Test regex operations
        let patterns = vec![
            r"/api/v[0-9]+/users/[0-9]+",
            r".*token=.*",
            r"/health/.*",
        ];
        
        for pattern in patterns {
            let _result = crate::performance::get_cached_regex(pattern)
                .unwrap()
                .is_match("/api/v1/users/123");
        }
        
        // Test header processing
        let headers = create_test_headers();
        let _filtered = crate::performance::filter_headers_optimized(&headers);
        
        let duration = start.elapsed();
        assert!(
            duration.as_millis() < 10,
            "Proxy overhead {}ms exceeds 10ms target",
            duration.as_millis()
        );
    }
    
    #[tokio::test]
    async fn test_regex_cache_performance() {
        let pattern = r"/api/v[0-9]+/users/[0-9]+";
        let test_path = "/api/v1/users/123";
        
        // First call (cache miss)
        let start = std::time::Instant::now();
        let regex1 = crate::performance::get_cached_regex(pattern).unwrap();
        let miss_duration = start.elapsed();
        
        // Second call (cache hit)
        let start = std::time::Instant::now();
        let regex2 = crate::performance::get_cached_regex(pattern).unwrap();
        let hit_duration = start.elapsed();
        
        // Verify they work the same
        assert_eq!(regex1.is_match(test_path), regex2.is_match(test_path));
        
        // Cache hit should be much faster
        assert!(
            hit_duration.as_nanos() < miss_duration.as_nanos() / 2,
            "Cache hit should be at least 2x faster than compilation"
        );
        
        // Verify metrics
        let stats = crate::performance::REGEX_METRICS.get_stats();
        assert!(stats.cache_hits >= 1);
        assert!(stats.cache_misses >= 1);
        assert!(stats.hit_rate() > 0.0);
    }
    
    #[tokio::test]
    async fn test_header_processing_performance() {
        let headers = create_test_headers();
        
        // Test optimized header processing
        let start = std::time::Instant::now();
        let _filtered = crate::performance::filter_headers_optimized(&headers);
        let duration = start.elapsed();
        
        // Header processing should be under 1ms
        assert!(
            duration.as_millis() < 1,
            "Header processing took {}ms, should be under 1ms",
            duration.as_millis()
        );
    }
    
    #[tokio::test]
    async fn test_config_snapshot_performance() {
        let config = create_test_config();
        let req = create_test_request();
        
        let start = std::time::Instant::now();
        let _snapshot = crate::performance::ConfigSnapshot::from_config(&config, &req);
        let duration = start.elapsed();
        
        // Config snapshot creation should be under 1ms
        assert!(
            duration.as_millis() < 1,
            "Config snapshot creation took {}ms, should be under 1ms",
            duration.as_millis()
        );
    }
    
    #[tokio::test]
    async fn test_performance_metrics() {
        let metrics = crate::performance::PERFORMANCE_METRICS;
        
        // Record some sample performance data
        metrics.record_request(Duration::from_millis(5));
        metrics.record_request(Duration::from_millis(8));
        metrics.record_request(Duration::from_millis(3));
        
        let stats = metrics.get_stats();
        
        assert_eq!(stats.total_requests, 3);
        assert_eq!(stats.avg_overhead_ms, 16.0 / 3.0); // (5+8+3)/3
        assert_eq!(stats.max_overhead_ms, 8.0);
    }
    
    #[tokio::test]
    async fn test_string_cache_performance() {
        let test_string = "test_string_for_caching";
        
        // First call (cache miss)
        let start = std::time::Instant::now();
        let cached1 = crate::performance::get_cached_string(test_string);
        let miss_duration = start.elapsed();
        
        // Second call (cache hit)
        let start = std::time::Instant::now();
        let cached2 = crate::performance::get_cached_string(test_string);
        let hit_duration = start.elapsed();
        
        assert_eq!(cached1, cached2);
        assert_eq!(cached1, test_string);
        
        // Cache hit should be faster (though string allocation is fast anyway)
        assert!(hit_duration <= miss_duration);
    }
    
    fn create_test_config() -> Config {
        Config {
            server: ServerConfig { port: 3000 },
            logging: LoggingConfig {
                default: false,
                rules: vec![
                    LoggingRule {
                        name: "test_rule".to_string(),
                        match_conditions: MatchConditions {
                            path: PathMatch {
                                patterns: vec![r"/api/v[0-9]+/.*".to_string()],
                            },
                            methods: vec!["GET".to_string(), "POST".to_string()],
                            headers: std::collections::HashMap::new(),
                            body: BodyMatch { patterns: vec![] },
                        },
                        capture: CaptureConfig {
                            headers: vec![],
                            body: true,
                            method: true,
                            path: true,
                            timing: true,
                        },
                        timeout: Some("5000ms".to_string()),
                    }
                ],
            },
            drop: DropConfig {
                default: false,
                rules: vec![],
            },
        }
    }
    
    fn create_test_request() -> axum::extract::Request {
        use axum::body::Body;
        use axum::http::{Method, Uri};
        
        axum::extract::Request::builder()
            .method(Method::GET)
            .uri("/api/v1/users/123")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap()
    }
    
    fn create_test_headers() -> axum::http::HeaderMap {
        use axum::http::{HeaderValue, HeaderName};
        
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("content-type", "application/json");
        headers.insert("authorization", "Bearer token123");
        headers.insert("user-agent", "Test-Agent/1.0");
        headers.insert("accept", "application/json");
        headers.insert("x-custom-header", "custom-value");
        
        headers
    }
}