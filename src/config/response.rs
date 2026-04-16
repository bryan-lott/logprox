use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::request::BodyMatch;

/// Controls response logging. Set `default: true` to log all responses, or define `rules`
/// to log only matching ones. First matching rule wins.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ResponseLoggingConfig {
    /// Log all responses when no rule matches.
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub rules: Vec<ResponseLoggingRule>,
}

/// A rule that logs matching upstream responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseLoggingRule {
    pub name: String,
    pub match_conditions: ResponseMatchConditions,
    pub capture: ResponseCaptureConfig,
}

/// Conditions that must all be satisfied for a response logging rule to match.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMatchConditions {
    /// HTTP status codes — response status must appear in the list. Empty = match any status.
    #[serde(default)]
    pub status_codes: Vec<u16>,
    /// Header conditions — all specified headers must match their regex pattern (AND).
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Body regex patterns — at least one must match (OR). Empty = match any body.
    #[serde(default)]
    pub body: BodyMatch,
}

/// Specifies what response data to include in log output.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResponseCaptureConfig {
    /// Response header names to capture.
    #[serde(default)]
    pub headers: Vec<String>,
    #[serde(default)]
    pub body: bool,
    #[serde(default)]
    pub status_code: bool,
    /// Log elapsed time from request receipt to response completion.
    #[serde(default)]
    pub timing: bool,
}
