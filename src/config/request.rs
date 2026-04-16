use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Controls request logging. Set `default: true` to log all requests, or define `rules`
/// to log only matching ones. First matching rule wins.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log all requests when no rule matches.
    pub default: bool,
    pub rules: Vec<LoggingRule>,
}

/// Controls request dropping. Set `default: true` to drop all requests, or define `rules`
/// to drop only matching ones. First matching rule wins.
#[derive(Debug, Serialize, Deserialize)]
pub struct DropConfig {
    /// Drop all requests when no rule matches (returns 403).
    pub default: bool,
    pub rules: Vec<DropRule>,
}

/// A rule that drops matching requests and returns a fixed response.
#[derive(Debug, Serialize, Deserialize)]
pub struct DropRule {
    pub name: String,
    pub match_conditions: MatchConditions,
    pub response: DropResponse,
}

/// The HTTP response returned when a drop rule matches.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DropResponse {
    pub status_code: u16,
    /// Response body. Supports `${ENV_VAR}` substitution.
    #[serde(default)]
    pub body: Option<String>,
}

/// A rule that logs matching requests. Optionally applies a per-request upstream timeout.
#[derive(Debug, Serialize, Deserialize)]
pub struct LoggingRule {
    pub name: String,
    pub match_conditions: MatchConditions,
    pub capture: CaptureConfig,
    /// Upstream timeout for requests matching this rule (e.g. `"30s"`, `"500ms"`).
    /// No timeout applied if absent.
    #[serde(default)]
    pub timeout: Option<String>,
}

impl LoggingRule {
    /// Parses the `timeout` string into a [`Duration`](std::time::Duration).
    /// Returns `None` if `timeout` is absent or has an unrecognised format.
    pub fn parse_timeout(&self) -> Option<std::time::Duration> {
        self.timeout.as_deref().and_then(parse_duration_str)
    }
}

fn parse_duration_str(s: &str) -> Option<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(suffix) = s.strip_suffix("ms") {
        suffix.trim().parse::<u64>().ok().map(std::time::Duration::from_millis)
    } else if let Some(suffix) = s.strip_suffix('s') {
        suffix.trim().parse::<u64>().ok().map(std::time::Duration::from_secs)
    } else {
        None
    }
}

/// Conditions that must all be satisfied for a rule to match a request.
/// Empty collections mean "match anything" for that condition.
/// Different condition types are ANDed; within path/body pattern lists, any one match suffices (OR).
#[derive(Debug, Serialize, Deserialize)]
pub struct MatchConditions {
    /// Path regex patterns — at least one must match (OR). Empty = match any path.
    #[serde(default)]
    pub path: PathMatch,
    /// HTTP methods — request method must appear in the list. Empty = match any method.
    #[serde(default)]
    pub methods: Vec<String>,
    /// Header conditions — all specified headers must match their regex pattern (AND).
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Body regex patterns — at least one must match (OR). Empty = match any body.
    #[serde(default)]
    pub body: BodyMatch,
}

/// Regex patterns matched against the request path.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PathMatch {
    pub patterns: Vec<String>,
}

/// Regex patterns matched against the request body.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BodyMatch {
    pub patterns: Vec<String>,
}

/// Specifies what request data to include in log output.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CaptureConfig {
    /// Header names to capture.
    #[serde(default)]
    pub headers: Vec<String>,
    #[serde(default)]
    pub body: bool,
    #[serde(default)]
    pub method: bool,
    #[serde(default)]
    pub path: bool,
    /// Log elapsed time from request receipt to upstream response.
    #[serde(default)]
    pub timing: bool,
}
