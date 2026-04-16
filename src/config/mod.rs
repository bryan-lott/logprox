use parking_lot::{RwLock, RwLockReadGuard};
use std::sync::LazyLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub mod request;
pub mod response;

pub use request::*;
pub use response::*;

// ---------------------------------------------------------------------------
// Global regex cache — compiled once, reused across all requests and threads.
// parking_lot::RwLock: many concurrent readers, rare writes (only on first
// encounter of a new pattern after a config reload).
// ---------------------------------------------------------------------------
static REGEX_CACHE: LazyLock<RwLock<HashMap<String, Arc<regex::Regex>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Returns a cached compiled regex for `pattern`, compiling and caching it on
/// first call. Returns `None` for invalid patterns (same behaviour as before:
/// silently non-matching rather than panicking).
fn get_cached_regex(pattern: &str) -> Option<Arc<regex::Regex>> {
    // Fast path: pattern already compiled.
    {
        let cache = REGEX_CACHE.read();
        if let Some(re) = cache.get(pattern) {
            return Some(Arc::clone(re));
        }
    }
    // Slow path: compile and insert.
    let re = Arc::new(regex::Regex::new(pattern).ok()?);
    REGEX_CACHE.write().insert(pattern.to_string(), Arc::clone(&re));
    Some(re)
}

// ---------------------------------------------------------------------------
// Config structs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    3000
}

/// Controls which upstream targets the proxy is allowed to reach.
/// Default: http/https only, private/loopback IPs blocked (secure default).
/// Set `allow_private_networks: true` when proxying to internal services.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpstreamConfig {
    /// Permit requests to private/loopback/link-local IP ranges.
    /// Default: false. Enable when proxying to internal services.
    #[serde(default)]
    pub allow_private_networks: bool,
    /// URL schemes allowed. Default: ["http", "https"].
    #[serde(default = "default_allowed_schemes")]
    pub allowed_schemes: Vec<String>,
    /// If non-empty, only these hostnames/IPs are permitted (exact match).
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Hostnames/IPs always blocked regardless of other settings.
    #[serde(default)]
    pub denied_hosts: Vec<String>,
}

fn default_allowed_schemes() -> Vec<String> {
    vec!["http".to_string(), "https".to_string()]
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            allow_private_networks: false,
            allowed_schemes: default_allowed_schemes(),
            allowed_hosts: vec![],
            denied_hosts: vec![],
        }
    }
}

/// Top-level configuration loaded from a YAML file.
///
/// Load with [`Config::from_file`], then wrap in [`ConfigHolder`] to serve traffic.
/// All sections except `logging` and `drop` are optional and default to safe values.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub drop: DropConfig,
    #[serde(default)]
    pub response_logging: ResponseLoggingConfig,
    /// Upstream access controls (SSRF protection).
    #[serde(default)]
    pub upstream: UpstreamConfig,
}

/// Thread-safe wrapper around [`Config`] that supports hot reload.
///
/// Uses a `parking_lot::RwLock` internally — many concurrent readers, rare writes
/// (only during [`reload`](ConfigHolder::reload)). Pass as `Arc<ConfigHolder>` axum state.
#[derive(Debug)]
pub struct ConfigHolder {
    config: RwLock<Config>,
}

impl ConfigHolder {
    /// Creates a new `ConfigHolder`, pre-warming the regex cache for all patterns.
    pub fn new(config: Config) -> Self {
        // Pre-warm the global regex cache for all patterns in this config so
        // that the first live request does not pay compilation cost.
        prewarm_regex_cache(&config);
        Self {
            config: RwLock::new(config),
        }
    }

    pub fn reload(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_file =
            std::env::var("CONFIG_FILE").unwrap_or_else(|_| "config.yaml".to_string());
        let new_config = Config::from_file(&config_file)?;
        let mut config = self.config.write();
        *config = new_config;
        Ok(())
    }

    pub fn get(&self) -> RwLockReadGuard<'_, Config> {
        self.config.read()
    }
}

/// Pre-warm the global regex cache with every pattern in the config.
/// Invalid patterns are silently skipped (they will never match).
fn prewarm_regex_cache(config: &Config) {
    let all_patterns = config.logging.rules.iter()
        .flat_map(|r| {
            r.match_conditions.path.patterns.iter()
                .chain(r.match_conditions.body.patterns.iter())
                .chain(r.match_conditions.headers.values())
        })
        .chain(config.drop.rules.iter().flat_map(|r| {
            r.match_conditions.path.patterns.iter()
                .chain(r.match_conditions.body.patterns.iter())
                .chain(r.match_conditions.headers.values())
        }))
        .chain(config.response_logging.rules.iter().flat_map(|r| {
            r.match_conditions.body.patterns.iter()
                .chain(r.match_conditions.headers.values())
        }));

    let mut cache = REGEX_CACHE.write();
    for pattern in all_patterns {
        if !cache.contains_key(pattern) {
            if let Ok(re) = regex::Regex::new(pattern) {
                cache.insert(pattern.clone(), Arc::new(re));
            }
        }
    }
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let f = std::fs::File::open(path)?;
        let mut config: Config = serde_norway::from_reader(f)?;
        config.substitute_env_vars();
        // Validate all patterns at startup to surface bad regex before serving traffic.
        config.validate_patterns()?;
        // Pre-warm cache so first request pays no compilation cost.
        prewarm_regex_cache(&config);
        Ok(config)
    }

    fn validate_patterns(&self) -> Result<(), Box<dyn std::error::Error>> {
        for rule in &self.logging.rules {
            for p in &rule.match_conditions.path.patterns {
                regex::Regex::new(p).map_err(|e| format!("Invalid path pattern '{}': {}", p, e))?;
            }
            for p in &rule.match_conditions.body.patterns {
                regex::Regex::new(p).map_err(|e| format!("Invalid body pattern '{}': {}", p, e))?;
            }
            for p in rule.match_conditions.headers.values() {
                regex::Regex::new(p).map_err(|e| format!("Invalid header pattern '{}': {}", p, e))?;
            }
        }
        for rule in &self.drop.rules {
            for p in &rule.match_conditions.path.patterns {
                regex::Regex::new(p).map_err(|e| format!("Invalid path pattern '{}': {}", p, e))?;
            }
            for p in &rule.match_conditions.body.patterns {
                regex::Regex::new(p).map_err(|e| format!("Invalid body pattern '{}': {}", p, e))?;
            }
            for p in rule.match_conditions.headers.values() {
                regex::Regex::new(p).map_err(|e| format!("Invalid header pattern '{}': {}", p, e))?;
            }
        }
        for rule in &self.response_logging.rules {
            for p in &rule.match_conditions.body.patterns {
                regex::Regex::new(p).map_err(|e| format!("Invalid body pattern '{}': {}", p, e))?;
            }
            for p in rule.match_conditions.headers.values() {
                regex::Regex::new(p).map_err(|e| format!("Invalid header pattern '{}': {}", p, e))?;
            }
        }
        Ok(())
    }

    /// Match `text` against `pattern` using the global compiled-regex cache.
    fn match_pattern(pattern: &str, text: &str) -> bool {
        get_cached_regex(pattern)
            .map(|re| re.is_match(text))
            .unwrap_or(false)
    }

    fn substitute_env_vars(&mut self) {
        for rule in &mut self.drop.rules {
            if let Some(ref mut body) = rule.response.body {
                *body = Self::substitute_env_in_string(body);
            }
        }
    }

    pub fn substitute_env_in_string(s: &str) -> String {
        let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
        re.replace_all(s, |caps: &regex::Captures| {
            let var_name = &caps[1];
            std::env::var(var_name).unwrap_or_else(|_| format!("${{{}}}", var_name))
        })
        .to_string()
    }

    // -----------------------------------------------------------------------
    // Public matching methods — Request-based (used by tests and direct callers)
    // -----------------------------------------------------------------------

    pub fn should_log_request(
        &self,
        req: &axum::extract::Request,
        body_content: &str,
    ) -> Option<&CaptureConfig> {
        self.should_log_request_parts(req.method().as_str(), req.uri().path(), req.headers(), body_content)
    }

    pub fn should_drop_request(
        &self,
        req: &axum::extract::Request,
        body_content: &str,
    ) -> Option<DropResponse> {
        self.should_drop_request_parts(req.method().as_str(), req.uri().path(), req.headers(), body_content)
    }

    pub fn matches_rule(
        &self,
        req: &axum::extract::Request,
        conditions: &MatchConditions,
        body_content: &str,
    ) -> bool {
        self.matches_conditions_parts(req.method().as_str(), req.uri().path(), req.headers(), body_content, conditions)
    }

    // -----------------------------------------------------------------------
    // Part-based variants — used by proxy_handler after the body is consumed
    // -----------------------------------------------------------------------

    pub fn should_log_request_parts(
        &self,
        method: &str,
        path: &str,
        headers: &axum::http::HeaderMap,
        body_content: &str,
    ) -> Option<&CaptureConfig> {
        for rule in &self.logging.rules {
            if self.matches_conditions_parts(method, path, headers, body_content, &rule.match_conditions) {
                return Some(&rule.capture);
            }
        }
        if self.logging.default {
            static DEFAULT_CAPTURE: CaptureConfig = CaptureConfig {
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

    pub fn should_drop_request_parts(
        &self,
        method: &str,
        path: &str,
        headers: &axum::http::HeaderMap,
        body_content: &str,
    ) -> Option<DropResponse> {
        for rule in &self.drop.rules {
            if self.matches_conditions_parts(method, path, headers, body_content, &rule.match_conditions) {
                return Some(rule.response.clone());
            }
        }
        if self.drop.default {
            Some(DropResponse {
                status_code: 403,
                body: Some("Request dropped by default".to_string()),
            })
        } else {
            None
        }
    }

    pub fn matches_rule_parts(
        &self,
        method: &str,
        path: &str,
        headers: &axum::http::HeaderMap,
        body_content: &str,
        conditions: &MatchConditions,
    ) -> bool {
        self.matches_conditions_parts(method, path, headers, body_content, conditions)
    }

    fn matches_conditions_parts(
        &self,
        method: &str,
        path: &str,
        headers: &axum::http::HeaderMap,
        body_content: &str,
        conditions: &MatchConditions,
    ) -> bool {
        // Method check
        if !conditions.methods.is_empty()
            && !conditions.methods.iter().any(|m| m.eq_ignore_ascii_case(method))
        {
            return false;
        }

        // Path check
        if !conditions.path.patterns.is_empty()
            && !conditions.path.patterns.iter().any(|p| Self::match_pattern(p, path))
        {
            return false;
        }

        // Header check (all specified headers must match)
        for (header_name, pattern) in &conditions.headers {
            if let Some(value) = headers.get(header_name) {
                if let Ok(s) = value.to_str() {
                    if !Self::match_pattern(pattern, s) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }

        // Body check
        if !conditions.body.patterns.is_empty()
            && !conditions.body.patterns.iter().any(|p| Self::match_pattern(p, body_content))
        {
            return false;
        }

        true
    }

    pub fn should_log_response(
        &self,
        status_code: u16,
        headers: &axum::http::HeaderMap,
        body_content: &str,
    ) -> Option<&ResponseCaptureConfig> {
        for rule in &self.response_logging.rules {
            if self.matches_response_rule(status_code, headers, body_content, &rule.match_conditions) {
                return Some(&rule.capture);
            }
        }
        if self.response_logging.default {
            static DEFAULT_RESPONSE_CAPTURE: ResponseCaptureConfig = ResponseCaptureConfig {
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

    pub fn matches_response_rule(
        &self,
        status_code: u16,
        headers: &axum::http::HeaderMap,
        body_content: &str,
        conditions: &ResponseMatchConditions,
    ) -> bool {
        if !conditions.status_codes.is_empty() && !conditions.status_codes.contains(&status_code) {
            return false;
        }

        for (header_name, pattern) in &conditions.headers {
            if let Some(value) = headers.get(header_name) {
                if let Ok(s) = value.to_str() {
                    if !Self::match_pattern(pattern, s) {
                        return false;
                    }
                }
            } else {
                return false;
            }
        }

        if !conditions.body.patterns.is_empty()
            && !conditions.body.patterns.iter().any(|p| Self::match_pattern(p, body_content))
        {
            return false;
        }

        true
    }
}
