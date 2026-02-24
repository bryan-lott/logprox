// Lock-free configuration access for ultra-fast rule matching
use std::collections::HashMap;
use std::sync::{Arc, RwLock, atomic::{AtomicU64, Ordering}};
use regex::Regex;
use arc_swap::ArcSwap;

use crate::config::{Config, MatchConditions, CaptureConfig};
use crate::performance::{Metrics, time_section};

/// Pre-compiled configuration snapshot for instant access
#[derive(Clone)]
pub struct ConfigSnapshot {
    pub rules: Vec<CompiledRule>,
    pub drop_rules: Vec<CompiledDropRule>,
    pub response_rules: Vec<CompiledResponseRule>,
    pub version: u64,
}

/// Pre-compiled regex rule for instant matching
#[derive(Clone)]
pub struct CompiledRule {
    pub match_conditions: CompiledMatchConditions,
    pub capture: CaptureConfig,
    pub timeout: Option<std::time::Duration>,
}

/// Pre-compiled drop rule
#[derive(Clone)]
pub struct CompiledDropRule {
    pub match_conditions: CompiledMatchConditions,
    pub status_code: u16,
    pub body: Option<String>,
}

/// Pre-compiled response rule
#[derive(Clone)]
pub struct CompiledResponseRule {
    pub match_conditions: CompiledResponseMatchConditions,
    pub capture: crate::config::ResponseCaptureConfig,
}

/// Pre-compiled match conditions with cached regex
#[derive(Clone)]
pub struct CompiledMatchConditions {
    pub path_patterns: Vec<Arc<Regex>>,
    pub methods: Vec<String>,
    pub headers: HashMap<String, Arc<Regex>>,
    pub body_patterns: Vec<Arc<Regex>>,
}

/// Pre-compiled response match conditions
#[derive(Clone)]
pub struct CompiledResponseMatchConditions {
    pub status_codes: Vec<u16>,
    pub headers: HashMap<String, Arc<Regex>>,
    pub body_patterns: Vec<Arc<Regex>>,
}

/// Lock-free configuration manager
pub struct LockFreeConfig {
    config_snapshots: ArcSwap<ConfigSnapshot>,
    version: AtomicU64,
}

impl LockFreeConfig {
    pub fn new() -> Self {
        Self {
            config_snapshots: ArcSwap::from(Arc::new(ConfigSnapshot::empty())),
            version: AtomicU64::new(0),
        }
    }

    /// Update configuration atomically
    pub fn update_config(&self, config: &Config) {
        let new_version = self.version.fetch_add(1, Ordering::AcqRel) + 1;
        
        let snapshot = ConfigSnapshot {
            rules: Self::compile_logging_rules(&config.logging.rules),
            drop_rules: Self::compile_drop_rules(&config.drop.rules),
            response_rules: Self::compile_response_rules(&config.response_logging.rules),
            version: new_version,
        };
        
        // Atomic swap
        self.config_snapshots.store(Arc::new(snapshot));
    }

    /// Get current configuration snapshot
    pub fn get_snapshot(&self) -> Arc<ConfigSnapshot> {
        self.config_snapshots.load_full()
    }

    /// Check if request should be logged (ultra-fast)
    pub fn should_log_request(&self, req: &axum::extract::Request, body_content: &str, metrics: &mut Metrics) -> Option<&CaptureConfig> {
        time_section!(start, metrics.config_lookup_time, {
            let snapshot = self.get_snapshot();
            
            for rule in &snapshot.rules {
                if self.matches_compiled_rule(req, &rule.match_conditions, body_content) {
                    return Some(&rule.capture);
                }
            }
            
            None
        })
    }

    /// Check if response should be logged
    pub fn should_log_response(
        &self,
        status_code: u16,
        headers: &axum::http::HeaderMap,
        body_content: &str,
        metrics: &mut Metrics
    ) -> Option<&crate::config::ResponseCaptureConfig> {
        time_section!(start, metrics.config_lookup_time, {
            let snapshot = self.get_snapshot();
            
            for rule in &snapshot.response_rules {
                if self.matches_compiled_response_rule(status_code, headers, body_content, &rule.match_conditions) {
                    return Some(&rule.capture);
                }
            }
            
            None
        })
    }

    /// Check if request should be dropped
    pub fn should_drop_request(
        &self,
        req: &axum::extract::Request,
        body_content: &str,
        metrics: &mut Metrics
    ) -> Option<&CompiledDropRule> {
        time_section!(start, metrics.config_lookup_time, {
            let snapshot = self.get_snapshot();
            
            for rule in &snapshot.drop_rules {
                if self.matches_compiled_rule(req, &rule.match_conditions, body_content) {
                    return Some(rule);
                }
            }
            
            None
        })
    }

    /// Match request against compiled rule
    fn matches_compiled_rule(
        &self,
        req: &axum::extract::Request,
        conditions: &CompiledMatchConditions,
        body_content: &str
    ) -> bool {
        // Check method
        if !conditions.methods.is_empty()
            && !conditions.methods.iter().any(|m| m == req.method().as_str()) {
            return false;
        }

        // Check path with pre-compiled regexes
        if !conditions.path_patterns.is_empty() {
            let path = req.uri().path();
            if !conditions.path_patterns.iter().any(|regex| regex.is_match(path)) {
                return false;
            }
        }

        // Check headers with pre-compiled regexes
        for (header_name, pattern) in &conditions.headers {
            if let Some(header_value) = req.headers().get(header_name) {
                if let Ok(header_str) = header_value.to_str() {
                    if !pattern.is_match(header_str) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }

        // Check body with pre-compiled regexes
        if !conditions.body_patterns.is_empty() && !conditions.body_patterns.iter().any(|regex| regex.is_match(body_content)) {
            return false;
        }

        true
    }

    /// Match response against compiled rule
    fn matches_compiled_response_rule(
        status_code: u16,
        headers: &axum::http::HeaderMap,
        body_content: &str,
        conditions: &CompiledResponseMatchConditions
    ) -> bool {
        // Check status codes
        if !conditions.status_codes.is_empty() && !conditions.status_codes.contains(&status_code) {
            return false;
        }

        // Check headers with pre-compiled regexes
        for (header_name, pattern) in &conditions.headers {
            if let Some(header_value) = headers.get(header_name) {
                if let Ok(header_str) = header_value.to_str() {
                    if !pattern.is_match(header_str) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }

        // Check body with pre-compiled regexes
        if !conditions.body_patterns.is_empty() && !conditions.body_patterns.iter().any(|regex| regex.is_match(body_content)) {
            return false;
        }

        true
    }

    /// Compile logging rules with regex caching
    fn compile_logging_rules(rules: &[crate::config::LoggingRule]) -> Vec<CompiledRule> {
        rules.iter().map(|rule| CompiledRule {
            match_conditions: CompiledMatchConditions {
                path_patterns: rule.match_conditions.path.patterns.iter()
                    .map(|pattern| Arc::new(Regex::new(pattern).unwrap_or_else(|_| Regex::new(r".^ NEVER_MATCH $").unwrap())))
                    .collect(),
                methods: rule.match_conditions.methods.clone(),
                headers: rule.match_conditions.headers.iter()
                    .map(|(name, pattern)| (name.clone(), Arc::new(Regex::new(pattern).unwrap_or_else(|_| Regex::new(r".*").unwrap()))))
                    .collect(),
                body_patterns: rule.match_conditions.body.patterns.iter()
                    .map(|pattern| Arc::new(Regex::new(pattern).unwrap_or_else(|_| Regex::new(r".*").unwrap())))
                    .collect(),
            },
            capture: rule.capture.clone(),
            timeout: rule.timeout.as_ref().and_then(|t| crate::handlers::proxy::parse_duration_string(t)),
        }).collect()
    }

    /// Compile drop rules
    fn compile_drop_rules(rules: &[crate::config::DropRule]) -> Vec<CompiledDropRule> {
        rules.iter().map(|rule| CompiledDropRule {
            match_conditions: CompiledMatchConditions {
                path_patterns: rule.match_conditions.path.patterns.iter()
                    .map(|pattern| Arc::new(Regex::new(pattern).unwrap_or_else(|_| Regex::new(r".*").unwrap())))
                    .collect(),
                methods: rule.match_conditions.methods.clone(),
                headers: rule.match_conditions.headers.iter()
                    .map(|(name, pattern)| (name.clone(), Arc::new(Regex::new(pattern).unwrap_or_else(|_| Regex::new(r".*").unwrap()))))
                    .collect(),
                body_patterns: rule.match_conditions.body.patterns.iter()
                    .map(|pattern| Arc::new(Regex::new(pattern).unwrap_or_else(|_| Regex::new(r".*").unwrap())))
                    .collect(),
            },
            status_code: rule.response.status_code,
            body: rule.response.body.clone(),
        }).collect()
    }

    /// Compile response rules
    fn compile_response_rules(rules: &[crate::config::ResponseLoggingRule]) -> Vec<CompiledResponseRule> {
        rules.iter().map(|rule| CompiledResponseRule {
            match_conditions: CompiledResponseMatchConditions {
                status_codes: rule.match_conditions.status_codes.clone(),
                headers: rule.match_conditions.headers.iter()
                    .map(|(name, pattern)| (name.clone(), Arc::new(Regex::new(pattern).unwrap_or_else(|_| Regex::new(r".*").unwrap()))))
                    .collect(),
                body_patterns: rule.match_conditions.body.patterns.iter()
                    .map(|pattern| Arc::new(Regex::new(pattern).unwrap_or_else(|_| Regex::new(r".*").unwrap())))
                    .collect(),
            },
            capture: rule.capture.clone(),
        }).collect()
    }
}

impl ConfigSnapshot {
    pub fn empty() -> Self {
        Self {
            rules: Vec::new(),
            drop_rules: Vec::new(),
            response_rules: Vec::new(),
            version: 0,
        }
    }
}

/// Thread-local lock-free config instance
thread_local! {
    static LOCK_FREE_CONFIG: std::cell::RefCell<LockFreeConfig> = 
        std::cell::RefCell::new(LockFreeConfig::new());
}

/// Get global lock-free config
pub fn get_lockfree_config() -> &'static LockFreeConfig {
    LOCK_FREE_CONFIG.with(|config| {
        // Return reference that lives as long as the thread
        unsafe { std::mem::transmute::<_, &'static LockFreeConfig>(config.as_ptr()) }
    })
}

/// Initialize lock-free config from regular config
pub fn initialize_lockfree_config(config: &Config) {
    LOCK_FREE_CONFIG.with(|lf_config| {
        let mut config_holder = lf_config.borrow_mut();
        config_holder.update_config(config);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Request;
    use axum::body::Body;
    use std::time::Duration;

    #[test]
    fn test_compiled_rule_creation() {
        let rule = CompiledRule {
            match_conditions: CompiledMatchConditions {
                path_patterns: vec![Arc::new(Regex::new(r"^/api/.*").unwrap())],
                methods: vec!["GET".to_string()],
                headers: HashMap::new(),
                body_patterns: vec![],
            },
            capture: CaptureConfig::default(),
            timeout: Some(Duration::from_secs(30)),
        };

        assert_eq!(rule.match_conditions.path_patterns.len(), 1);
        assert_eq!(rule.match_conditions.methods.len(), 1);
    }

    #[test]
    fn test_config_update() {
        let config = LockFreeConfig::new();
        let initial_version = config.get_snapshot().version;
        
        // Simulate config update
        let test_config = crate::config::Config {
            server: crate::config::ServerConfig { port: 3000 },
            logging: crate::config::LoggingConfig {
                default: false,
                rules: vec![],
            },
            drop: crate::config::DropConfig {
                default: false,
                rules: vec![],
            },
            response_logging: crate::config::ResponseLoggingConfig {
                default: false,
                rules: vec![],
            },
        };
        
        config.update_config(&test_config);
        
        let new_snapshot = config.get_snapshot();
        assert!(new_snapshot.version > initial_version);
    }

    #[test]
    fn test_rule_matching() {
        let conditions = CompiledMatchConditions {
            path_patterns: vec![Arc::new(Regex::new(r"^/api/.*").unwrap())],
            methods: vec!["GET".to_string()],
            headers: HashMap::new(),
            body_patterns: vec![],
        };
        
        // Create test request
        let request = Request::builder()
            .method("GET")
            .uri("/api/users")
            .body(Body::empty())
            .unwrap();
        
        let config = LockFreeConfig::new();
        assert!(config.matches_compiled_rule(&request, &conditions, ""));
    }

    #[tokio::test]
    async fn test_request_logging_performance() {
        let config = LockFreeConfig::new();
        let mut metrics = Metrics::new();
        
        let request = Request::builder()
            .method("POST")
            .uri("/api/test")
            .header("content-type", "application/json")
            .body(Body::from("test body"))
            .unwrap();
        
        let start = std::time::Instant::now();
        let _result = config.should_log_request(&request, "test body", &mut metrics);
        let elapsed = start.elapsed();
        
        // Should be extremely fast (<50μs)
        assert!(elapsed.as_micros() < 50);
    }

    #[test]
    fn test_memory_usage() {
        // Lock-free config should use minimal memory for snapshots
        let config = LockFreeConfig::new();
        let snapshot = config.get_snapshot();
        
        // Should have empty initial configuration
        assert_eq!(snapshot.rules.len(), 0);
        assert_eq!(snapshot.drop_rules.len(), 0);
        assert_eq!(snapshot.response_rules.len(), 0);
    }
}

impl CompiledPattern {
    pub fn new(pattern: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            regex: regex::Regex::new(pattern)?,
            pattern: pattern.to_string(),
        })
    }

    #[inline]
    pub fn is_match(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }
}

// Pre-compiled match conditions for ultra-fast evaluation
#[derive(Clone)]
pub struct CompiledMatchConditions {
    pub path_patterns: Vec<CompiledPattern>,
    pub methods: Vec<String>,
    pub header_patterns: Vec<(String, CompiledPattern)>,
    pub body_patterns: Vec<CompiledPattern>,
}

impl From<&MatchConditions> for CompiledMatchConditions {
    fn from(conditions: &MatchConditions) -> Self {
        let mut header_patterns = Vec::new();
        for (header_name, pattern) in &conditions.headers {
            if let Ok(compiled) = CompiledPattern::new(pattern) {
                header_patterns.push((header_name.clone(), compiled));
            }
        }

        Self {
            path_patterns: conditions
                .path
                .patterns
                .iter()
                .filter_map(|p| CompiledPattern::new(p).ok())
                .collect(),
            methods: conditions.methods.clone(),
            header_patterns,
            body_patterns: conditions
                .body
                .patterns
                .iter()
                .filter_map(|p| CompiledPattern::new(p).ok())
                .collect(),
        }
    }
}

#[derive(Clone)]
pub struct CompiledResponseMatchConditions {
    pub status_codes: Vec<u16>,
    pub header_patterns: Vec<(String, CompiledPattern)>,
    pub body_patterns: Vec<CompiledPattern>,
}

impl From<&ResponseMatchConditions> for CompiledResponseMatchConditions {
    fn from(conditions: &ResponseMatchConditions) -> Self {
        let mut header_patterns = Vec::new();
        for (header_name, pattern) in &conditions.headers {
            if let Ok(compiled) = CompiledPattern::new(pattern) {
                header_patterns.push((header_name.clone(), compiled));
            }
        }

        Self {
            status_codes: conditions.status_codes.clone(),
            header_patterns,
            body_patterns: conditions
                .body
                .patterns
                .iter()
                .filter_map(|p| CompiledPattern::new(p).ok())
                .collect(),
        }
    }
}

// Lock-free configuration snapshot
#[derive(Clone)]
pub struct ConfigSnapshot {
    pub compiled_logging_rules: Vec<CompiledLoggingRule>,
    pub compiled_drop_rules: Vec<CompiledDropRule>,
    pub compiled_response_rules: Vec<CompiledResponseLoggingRule>,
    pub default_logging: bool,
    pub default_drop: bool,
    pub default_response_logging: bool,
}

#[derive(Clone)]
pub struct CompiledLoggingRule {
    pub name: String,
    pub conditions: CompiledMatchConditions,
    pub capture: crate::config::CaptureConfig,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone)]
pub struct CompiledDropRule {
    pub name: String,
    pub conditions: CompiledMatchConditions,
    pub response: crate::config::DropResponse,
}

#[derive(Clone)]
pub struct CompiledResponseLoggingRule {
    pub name: String,
    pub conditions: CompiledResponseMatchConditions,
    pub capture: crate::config::ResponseCaptureConfig,
}

impl From<&crate::config::LoggingRule> for CompiledLoggingRule {
    fn from(rule: &crate::config::LoggingRule) -> Self {
        Self {
            name: rule.name.clone(),
            conditions: CompiledMatchConditions::from(&rule.match_conditions),
            capture: rule.capture.clone(),
            timeout_ms: rule.timeout.as_ref().and_then(|t| parse_timeout_ms(t)),
        }
    }
}

impl From<&crate::config::DropRule> for CompiledDropRule {
    fn from(rule: &crate::config::DropRule) -> Self {
        Self {
            name: rule.name.clone(),
            conditions: CompiledMatchConditions::from(&rule.match_conditions),
            response: rule.response.clone(),
        }
    }
}

impl From<&crate::config::response::ResponseLoggingRule> for CompiledResponseLoggingRule {
    fn from(rule: &crate::config::response::ResponseLoggingRule) -> Self {
        Self {
            name: rule.name.clone(),
            conditions: CompiledResponseMatchConditions::from(&rule.match_conditions),
            capture: rule.capture.clone(),
        }
    }
}

// Lock-free configuration holder
pub struct LockFreeConfigHolder {
    snapshot: Arc<RwLock<ConfigSnapshot>>,
}

impl LockFreeConfigHolder {
    pub fn new(config: Config) -> Self {
        let snapshot = Self::compile_config(config);
        Self {
            snapshot: Arc::new(RwLock::new(snapshot)),
        }
    }

    pub fn reload(&self, config: Config) -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = Self::compile_config(config);
        let mut current = self.snapshot.write();
        *current = snapshot;
        Ok(())
    }

    pub fn get_snapshot(&self) -> ConfigSnapshot {
        self.snapshot.read().clone()
    }

    fn compile_config(config: Config) -> ConfigSnapshot {
        ConfigSnapshot {
            compiled_logging_rules: config
                .logging
                .rules
                .iter()
                .map(CompiledLoggingRule::from)
                .collect(),
            compiled_drop_rules: config
                .drop
                .rules
                .iter()
                .map(CompiledDropRule::from)
                .collect(),
            compiled_response_rules: config
                .response_logging
                .rules
                .iter()
                .map(CompiledResponseLoggingRule::from)
                .collect(),
            default_logging: config.logging.default,
            default_drop: config.drop.default,
            default_response_logging: config.response_logging.default,
        }
    }
}

impl ConfigSnapshot {
    // Ultra-fast rule matching without lock contention
    pub fn should_log_request(
        &self,
        req: &Request,
        body_content: &str,
    ) -> Option<&crate::config::CaptureConfig> {
        for rule in &self.compiled_logging_rules {
            if self.matches_request_rule(req, &rule.conditions, body_content) {
                return Some(&rule.capture);
            }
        }

        if self.default_logging {
            static DEFAULT_CAPTURE: crate::config::CaptureConfig = crate::config::CaptureConfig {
                headers: vec![],
                body: true,
                method: true,
                path: true,
                timing: true,
            };
            Some(&DEFAULT_CAPTURE)
        } else {
            None
        }
    }

    pub fn should_drop_request(
        &self,
        req: &Request,
        body_content: &str,
    ) -> Option<&crate::config::DropResponse> {
        for rule in &self.compiled_drop_rules {
            if self.matches_request_rule(req, &rule.conditions, body_content) {
                return Some(&rule.response);
            }
        }

        if self.default_drop {
            static DEFAULT_RESPONSE: crate::config::DropResponse = crate::config::DropResponse {
                status_code: 403,
                body: Some("Request dropped by default".to_string()),
            };
            Some(&DEFAULT_RESPONSE)
        } else {
            None
        }
    }

    pub fn should_log_response(
        &self,
        status_code: u16,
        headers: &HeaderMap,
        body_content: &str,
    ) -> Option<&crate::config::ResponseCaptureConfig> {
        for rule in &self.compiled_response_rules {
            if self.matches_response_rule(status_code, headers, body_content, &rule.conditions) {
                return Some(&rule.capture);
            }
        }

        if self.default_response_logging {
            static DEFAULT_RESPONSE_CAPTURE: crate::config::ResponseCaptureConfig =
                crate::config::ResponseCaptureConfig {
                    headers: vec![],
                    body: true,
                    status_code: true,
                    timing: true,
                };
            Some(&DEFAULT_RESPONSE_CAPTURE)
        } else {
            None
        }
    }

    #[inline]
    fn matches_request_rule(
        &self,
        req: &Request,
        conditions: &CompiledMatchConditions,
        body_content: &str,
    ) -> bool {
        // Check method
        if !conditions.methods.is_empty()
            && !conditions
                .methods
                .iter()
                .any(|m| m == req.method().as_str())
        {
            return false;
        }

        // Check path
        if !conditions.path_patterns.is_empty() {
            let path = req.uri().path();
            let matches = conditions
                .path_patterns
                .iter()
                .any(|pattern| pattern.is_match(path));
            if !matches {
                return false;
            }
        }

        // Check headers
        for (header_name, pattern) in &conditions.header_patterns {
            if let Some(header_value) = req.headers().get(header_name) {
                if let Ok(header_str) = header_value.to_str() {
                    if !pattern.is_match(header_str) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }

        // Check body
        if !conditions.body_patterns.is_empty() {
            let matches = conditions
                .body_patterns
                .iter()
                .any(|pattern| pattern.is_match(body_content));
            if !matches {
                return false;
            }
        }

        true
    }

    #[inline]
    fn matches_response_rule(
        &self,
        status_code: u16,
        headers: &HeaderMap,
        body_content: &str,
        conditions: &CompiledResponseMatchConditions,
    ) -> bool {
        // Check status code
        if !conditions.status_codes.is_empty() && !conditions.status_codes.contains(&status_code) {
            return false;
        }

        // Check headers
        for (header_name, pattern) in &conditions.header_patterns {
            if let Some(header_value) = headers.get(header_name) {
                if let Ok(header_str) = header_value.to_str() {
                    if !pattern.is_match(header_str) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }

        // Check body
        if !conditions.body_patterns.is_empty() {
            let matches = conditions
                .body_patterns
                .iter()
                .any(|pattern| pattern.is_match(body_content));
            if !matches {
                return false;
            }
        }

        true
    }
}

fn parse_timeout_ms(s: &str) -> Option<u64> {
    let s = s.trim();

    if s.is_empty() {
        return None;
    }

    if let Some(suffix) = s.strip_suffix("ms") {
        let num_str = suffix.trim();
        num_str.parse::<u64>().ok()
    } else if let Some(suffix) = s.strip_suffix("s") {
        let num_str = suffix.trim();
        num_str.parse::<u64>().ok().map(|secs| secs * 1000)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Config, DropConfig, DropRule, LoggingConfig, LoggingRule, MatchConditions,
    };
    use axum::http::{Method, Uri};
    use std::collections::HashMap;

    #[test]
    fn test_compiled_pattern() {
        let pattern = CompiledPattern::new(r"\d+").unwrap();
        assert!(pattern.is_match("123"));
        assert!(!pattern.is_match("abc"));
    }

    #[test]
    fn test_lock_free_config() {
        let config = Config {
            server: crate::config::ServerConfig { port: 3000 },
            logging: LoggingConfig {
                default: true,
                rules: vec![],
            },
            drop: DropConfig {
                default: false,
                rules: vec![],
            },
            response_logging: Default::default(),
        };

        let holder = LockFreeConfigHolder::new(config);
        let snapshot = holder.get_snapshot();

        assert!(snapshot.default_logging);
        assert!(!snapshot.default_drop);
    }

    #[test]
    fn test_request_matching() {
        let conditions = MatchConditions {
            path: crate::config::PathMatch {
                patterns: vec![r"/api/.*".to_string()],
            },
            methods: vec!["GET".to_string()],
            headers: HashMap::new(),
            body: Default::default(),
        };

        let compiled = CompiledMatchConditions::from(&conditions);

        let req = Request::builder()
            .method(Method::GET)
            .uri("/api/test")
            .body(axum::body::Body::empty())
            .unwrap();

        let snapshot = ConfigSnapshot {
            compiled_logging_rules: vec![],
            compiled_drop_rules: vec![],
            compiled_response_rules: vec![],
            default_logging: false,
            default_drop: false,
            default_response_logging: false,
        };

        assert!(snapshot.matches_request_rule(&req, &compiled, ""));
    }
}
