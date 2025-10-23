use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub default: bool,
    pub rules: Vec<LoggingRule>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DropConfig {
    pub default: bool,
    pub rules: Vec<DropRule>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DropRule {
    pub name: String,
    pub match_conditions: MatchConditions,
    pub response: DropResponse,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DropResponse {
    pub status_code: u16,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggingRule {
    pub name: String,
    pub match_conditions: MatchConditions,
    pub capture: CaptureConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchConditions {
    #[serde(default)]
    pub path: PathMatch,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: BodyMatch,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PathMatch {
    pub patterns: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BodyMatch {
    pub patterns: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CaptureConfig {
    #[serde(default)]
    pub headers: Vec<String>,
    #[serde(default)]
    pub body: bool,
    #[serde(default)]
    pub method: bool,
    #[serde(default)]
    pub path: bool,
    #[serde(default)]
    pub timing: bool,
}