use std::time::Duration;

#[test]
fn test_extract_upstream_url_valid() {
    assert_eq!(
        logprox::handlers::proxy::extract_upstream_url("/https://httpbin.org/anything").unwrap(),
        "https://httpbin.org/anything"
    );
    assert_eq!(
        logprox::handlers::proxy::extract_upstream_url("/http://example.com/test").unwrap(),
        "http://example.com/test"
    );
    assert_eq!(
        logprox::handlers::proxy::extract_upstream_url("/https://api.example.com/v1/users/123")
            .unwrap(),
        "https://api.example.com/v1/users/123"
    );
}

#[test]
fn test_extract_upstream_url_invalid() {
    assert!(logprox::handlers::proxy::extract_upstream_url("/").is_err());
    assert!(logprox::handlers::proxy::extract_upstream_url("").is_err());
    assert!(logprox::handlers::proxy::extract_upstream_url("/not-a-url").is_err());
}

#[test]
fn test_parse_duration_seconds() {
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("30s").unwrap(),
        Duration::from_secs(30)
    );
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("0s").unwrap(),
        Duration::from_secs(0)
    );
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("120s").unwrap(),
        Duration::from_secs(120)
    );
}

#[test]
fn test_parse_duration_milliseconds() {
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("5000ms").unwrap(),
        Duration::from_millis(5000)
    );
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("100ms").unwrap(),
        Duration::from_millis(100)
    );
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("0ms").unwrap(),
        Duration::from_millis(0)
    );
}

#[test]
fn test_parse_duration_invalid() {
    assert!(logprox::handlers::proxy::parse_duration_string("").is_none());
    assert!(logprox::handlers::proxy::parse_duration_string("30").is_none());
    assert!(logprox::handlers::proxy::parse_duration_string("30sec").is_none());
    assert!(logprox::handlers::proxy::parse_duration_string("abc").is_none());
}

#[test]
fn test_parse_duration_edge_cases() {
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("1s").unwrap(),
        Duration::from_secs(1)
    );
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("1ms").unwrap(),
        Duration::from_millis(1)
    );
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("3599s").unwrap(),
        Duration::from_secs(3599)
    );
    assert_eq!(
        logprox::handlers::proxy::parse_duration_string("999999ms").unwrap(),
        Duration::from_millis(999999)
    );
}
