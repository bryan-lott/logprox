use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::request::BodyMatch;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ResponseLoggingConfig {
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub rules: Vec<ResponseLoggingRule>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseLoggingRule {
    pub name: String,
    pub match_conditions: ResponseMatchConditions,
    pub capture: ResponseCaptureConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMatchConditions {
    #[serde(default)]
    pub status_codes: Vec<u16>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: BodyMatch,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResponseCaptureConfig {
    #[serde(default)]
    pub headers: Vec<String>,
    #[serde(default)]
    pub body: bool,
    #[serde(default)]
    pub status_code: bool,
    #[serde(default)]
    pub timing: bool,
}
