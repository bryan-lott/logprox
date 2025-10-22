use logprox::config::*;
use axum::http::{Method, Uri};

fn create_test_request(method: Method, path: &str, headers: Vec<(&str, &str)>) -> axum::extract::Request {
    let mut req_builder = axum::http::Request::builder()
        .method(method)
        .uri(Uri::try_from(path).unwrap());

    for (key, value) in headers {
        req_builder = req_builder.header(key, value);
    }

    req_builder.body(axum::body::Body::empty()).unwrap()
}

#[test]
fn test_config_yaml() {
    let config = Config::from_file("config.yaml").unwrap();

    // Verify server config
    assert_eq!(config.server.port, 3000);
    assert_eq!(config.server.config_file, "config.yaml");

    // Verify logging rules
    assert_eq!(config.logging.default, false);
    assert_eq!(config.logging.rules.len(), 3);

    // Verify drop rules
    assert_eq!(config.drop.default, false);
    assert_eq!(config.drop.rules.len(), 3);

    // Check first drop rule - deprecated API
    let deprecated_rule = &config.drop.rules[0];
    assert_eq!(deprecated_rule.name, "Drop deprecated API calls");
    assert_eq!(deprecated_rule.match_conditions.path.patterns, vec!["/api/v1/deprecated.*"]);
    assert_eq!(deprecated_rule.response.status_code, 410);
    assert_eq!(deprecated_rule.response.body, Some("This API endpoint has been deprecated and is no longer supported.".to_string()));

    // Check second drop rule - unauthorized
    let unauthorized_rule = &config.drop.rules[1];
    assert_eq!(unauthorized_rule.name, "Drop unauthorized requests");
    assert_eq!(unauthorized_rule.match_conditions.headers.get("authorization").unwrap(), ".*");
    assert_eq!(unauthorized_rule.match_conditions.path.patterns, vec!["/admin.*"]);
    assert_eq!(unauthorized_rule.response.status_code, 403);
    assert_eq!(unauthorized_rule.response.body, Some("Access denied.".to_string()));

    // Check first rule - API requests
    let api_rule = &config.logging.rules[0];
    assert_eq!(api_rule.name, "Log API requests");
    assert_eq!(api_rule.match_conditions.methods, vec!["POST", "PUT"]);
    assert_eq!(
        api_rule
            .match_conditions
            .headers
            .get("content-type")
            .unwrap(),
        "application/json.*"
    );
    assert!(api_rule
        .capture
        .headers
        .contains(&"content-type".to_string()));
    assert!(api_rule.capture.headers.contains(&"user-agent".to_string()));
    assert!(api_rule.capture.body);
    assert!(api_rule.capture.method);
    assert!(api_rule.capture.path);
    assert!(api_rule.capture.timing);

    // Check second rule - Health checks
    let health_rule = &config.logging.rules[1];
    assert_eq!(health_rule.name, "Log health checks");
    assert_eq!(
        health_rule.match_conditions.path.patterns,
        vec!["^/health$"]
    );
    assert!(health_rule.match_conditions.methods.is_empty());
    assert!(health_rule.match_conditions.headers.is_empty());
    assert!(health_rule.capture.timing);
    assert!(health_rule.capture.method);
    assert!(!health_rule.capture.body);
    assert!(!health_rule.capture.path);
    assert!(health_rule.capture.headers.is_empty());
}

#[test]
fn test_should_log_request() {
    let config = Config::from_file("config.yaml").unwrap();

    // Test matching API request
    let api_req = create_test_request(
        Method::POST,
        "/anything/test",
        vec![("content-type", "application/json")],
    );
    assert!(config.should_log_request(&api_req, "").is_some());

    // Test matching health check
    let health_req = create_test_request(Method::GET, "/health", vec![]);
    assert!(config.should_log_request(&health_req, "").is_some());

    // Test matching local test rule (matches .* path)
    let other_req = create_test_request(Method::GET, "/no-match", vec![]);
    assert!(config.should_log_request(&other_req, "").is_some());
}

#[test]
fn test_should_drop_request() {
    let config = Config::from_file("config.yaml").unwrap();

    // Test matching deprecated API
    let deprecated_req = create_test_request(Method::GET, "/api/v1/deprecated/old", vec![]);
    let drop_resp = config.should_drop_request(&deprecated_req, "").unwrap();
    assert_eq!(drop_resp.status_code, 410);
    assert!(drop_resp.body.is_some());

    // Test matching unauthorized admin
    let admin_req = create_test_request(
        Method::GET,
        "/admin/dashboard",
        vec![("authorization", "Bearer token")],
    );
    let drop_resp = config.should_drop_request(&admin_req, "").unwrap();
    assert_eq!(drop_resp.status_code, 403);

    // Test non-matching request
    let normal_req = create_test_request(Method::GET, "/api/v2/normal", vec![]);
    assert!(config.should_drop_request(&normal_req, "").is_none());
}

#[test]
fn test_matches_rule_method() {
    let config = Config::from_file("config.yaml").unwrap();

    // Test method match
    let post_req = create_test_request(Method::POST, "/test", vec![]);
    let conditions = MatchConditions {
        path: PathMatch { patterns: vec![] },
        methods: vec!["POST".to_string()],
        headers: std::collections::HashMap::new(),
        body: BodyMatch { patterns: vec![] },
    };
    assert!(config.matches_rule(&post_req, &conditions, ""));

    // Test method no match
    let get_req = create_test_request(Method::GET, "/test", vec![]);
    assert!(!config.matches_rule(&get_req, &conditions, ""));
}

#[test]
fn test_matches_rule_path() {
    let config = Config::from_file("config.yaml").unwrap();

    // Test path regex match
    let req = create_test_request(Method::GET, "/health", vec![]);
    let conditions = MatchConditions {
        path: PathMatch { patterns: vec!["^/health$".to_string()] },
        methods: vec![],
        headers: std::collections::HashMap::new(),
        body: BodyMatch { patterns: vec![] },
    };
    assert!(config.matches_rule(&req, &conditions, ""));

    // Test path no match
    let req2 = create_test_request(Method::GET, "/nothealth", vec![]);
    assert!(!config.matches_rule(&req2, &conditions, ""));
}

#[test]
fn test_matches_rule_headers() {
    let config = Config::from_file("config.yaml").unwrap();

    // Test header match
    let req = create_test_request(Method::GET, "/test", vec![("content-type", "application/json")]);
    let mut headers = std::collections::HashMap::new();
    headers.insert("content-type".to_string(), "application/json.*".to_string());
    let conditions = MatchConditions {
        path: PathMatch { patterns: vec![] },
        methods: vec![],
        headers,
        body: BodyMatch { patterns: vec![] },
    };
    assert!(config.matches_rule(&req, &conditions, ""));

    // Test header no match
    let req2 = create_test_request(Method::GET, "/test", vec![("content-type", "text/plain")]);
    assert!(!config.matches_rule(&req2, &conditions, ""));

    // Test missing header
    let req3 = create_test_request(Method::GET, "/test", vec![]);
    assert!(!config.matches_rule(&req3, &conditions, ""));
}

#[test]
fn test_matches_rule_combined_conditions() {
    let config = Config::from_file("config.yaml").unwrap();

    // Test all conditions match
    let req = create_test_request(
        Method::POST,
        "/anything/test",
        vec![("content-type", "application/json")],
    );
    let mut headers = std::collections::HashMap::new();
    headers.insert("content-type".to_string(), "application/json.*".to_string());
    let conditions = MatchConditions {
        path: PathMatch { patterns: vec!["/anything.*".to_string()] },
        methods: vec!["POST".to_string()],
        headers,
        body: BodyMatch { patterns: vec![] },
    };
    assert!(config.matches_rule(&req, &conditions, ""));

    // Test one condition fails
    let req2 = create_test_request(
        Method::GET,
        "/anything/test",
        vec![("content-type", "application/json")],
    );
    assert!(!config.matches_rule(&req2, &conditions, ""));
}

#[test]
fn test_matches_rule_body() {
    let config = Config::from_file("config.yaml").unwrap();

    // Test body regex match
    let req = create_test_request(Method::POST, "/test", vec![]);
    let conditions = MatchConditions {
        path: PathMatch { patterns: vec![] },
        methods: vec![],
        headers: std::collections::HashMap::new(),
        body: BodyMatch { patterns: vec![r#""amount":\s*\d+"#.to_string()] },
    };
    let body_with_amount = r#"{"amount": 123, "user": "test"}"#;
    assert!(config.matches_rule(&req, &conditions, body_with_amount));

    // Test body no match
    let body_without_amount = r#"{"user": "test", "status": "ok"}"#;
    assert!(!config.matches_rule(&req, &conditions, body_without_amount));

    // Test multiple patterns (any match)
    let conditions_multi = MatchConditions {
        path: PathMatch { patterns: vec![] },
        methods: vec![],
        headers: std::collections::HashMap::new(),
        body: BodyMatch { patterns: vec![r#"admin"#.to_string(), r#"secret"#.to_string()] },
    };
    assert!(config.matches_rule(&req, &conditions_multi, "user admin access"));
    assert!(config.matches_rule(&req, &conditions_multi, "contains secret data"));
    assert!(!config.matches_rule(&req, &conditions_multi, "normal user data"));
}

#[test]
fn test_should_drop_request_default() {
    // Create config with drop default true
    let config = Config {
        server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig {
            default: true,
            rules: vec![],
        },
    };

    let req = create_test_request(Method::GET, "/any", vec![]);
    let drop_resp = config.should_drop_request(&req, "").unwrap();
    assert_eq!(drop_resp.status_code, 403);
    assert!(drop_resp.body.is_some());
}

#[test]
fn test_config_from_file_invalid() {
    // Test with invalid YAML
    let result = Config::from_file("nonexistent.yaml");
    assert!(result.is_err());
}

#[test]
fn test_config_holder() {
    let initial_config = Config {
        server: ServerConfig { port: 3000, config_file: "config.yaml".to_string() },
        logging: LoggingConfig { default: false, rules: vec![] },
        drop: DropConfig { default: false, rules: vec![] },
    };
    let holder = ConfigHolder::new(initial_config);

    // Test getting config
    {
        let config = holder.get();
        assert_eq!(config.logging.default, false);
    }

    // Test reloading (should succeed with existing config.yaml)
    let reload_result = holder.reload();
    assert!(reload_result.is_ok());
}

#[test]
fn test_env_substitution() {
    std::env::set_var("TEST_SECRET", "replaced_value");
    let result = Config::substitute_env_in_string("prefix ${TEST_SECRET} suffix");
    assert_eq!(result, "prefix replaced_value suffix");

    // Test missing var
    let result2 = Config::substitute_env_in_string("no ${MISSING_VAR} here");
    assert_eq!(result2, "no ${MISSING_VAR} here");

    std::env::remove_var("TEST_SECRET");
}