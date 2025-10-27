use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_config_file")]
    pub config_file: String,
}

fn default_port() -> u16 { 3000 }
fn default_config_file() -> String { "config.yaml".to_string() }

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub drop: DropConfig,
    #[serde(default)]
    pub response_logging: ResponseLoggingConfig,
}

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

#[derive(Debug)]
pub struct ConfigHolder {
    config: RwLock<Config>,
}

impl ConfigHolder {
    pub fn new(config: Config) -> Self {
        Self {
            config: RwLock::new(config),
        }
    }

    pub fn reload(&self) -> Result<(), Box<dyn std::error::Error>> {
        let current_config = self.config.read().unwrap();
        let config_file = &current_config.server.config_file;
        let new_config = Config::from_file(config_file)?;
        drop(current_config); // Release the read lock
        let mut config = self.config.write().unwrap();
        *config = new_config;
        Ok(())
    }

    pub fn get(&self) -> std::sync::RwLockReadGuard<'_, Config> {
        self.config.read().unwrap()
    }
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let f = std::fs::File::open(path)?;
        let mut config: Config = serde_yaml::from_reader(f)?;
        config.substitute_env_vars();
        Ok(config)
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
        }).to_string()
    }

    pub fn should_log_request(&self, req: &axum::extract::Request, body_content: &str) -> Option<&CaptureConfig> {
        for rule in &self.logging.rules {
            if self.matches_rule(req, &rule.match_conditions, body_content) {
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

    pub fn should_drop_request(&self, req: &axum::extract::Request, body_content: &str) -> Option<DropResponse> {
        for rule in &self.drop.rules {
            if self.matches_rule(req, &rule.match_conditions, body_content) {
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

    pub fn matches_rule(&self, req: &axum::extract::Request, conditions: &MatchConditions, body_content: &str) -> bool {
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
        if !conditions.path.patterns.is_empty() {
            let path = req.uri().path();
            let matches = conditions.path.patterns.iter().any(|pattern| {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(path))
                    .unwrap_or(false)
            });
            if !matches {
                return false;
            }
        }

        // Check headers
        for (header_name, pattern) in &conditions.headers {
            if let Some(header_value) = req.headers().get(header_name) {
                if let Ok(header_str) = header_value.to_str() {
                    if let Ok(re) = regex::Regex::new(pattern) {
                        if !re.is_match(header_str) {
                            return false;
                        }
                    }
                }
            } else {
                return false;
            }
        }

        // Check body
        if !conditions.body.patterns.is_empty() {
            let matches = conditions.body.patterns.iter().any(|pattern| {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(body_content))
                    .unwrap_or(false)
            });
            if !matches {
                return false;
            }
        }

        true
    }

    pub fn should_log_response(&self, status_code: u16, headers: &axum::http::HeaderMap, body_content: &str) -> Option<&ResponseCaptureConfig> {
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

    pub fn matches_response_rule(&self, status_code: u16, headers: &axum::http::HeaderMap, body_content: &str, conditions: &ResponseMatchConditions) -> bool {
        // Check status code
        if !conditions.status_codes.is_empty() && !conditions.status_codes.contains(&status_code) {
            return false;
        }

        // Check headers
        for (header_name, pattern) in &conditions.headers {
            if let Some(header_value) = headers.get(header_name) {
                if let Ok(header_str) = header_value.to_str() {
                    if let Ok(re) = regex::Regex::new(pattern) {
                        if !re.is_match(header_str) {
                            return false;
                        }
                    }
                }
            } else {
                return false;
            }
        }

        // Check body
        if !conditions.body.patterns.is_empty() {
            let matches = conditions.body.patterns.iter().any(|pattern| {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(body_content))
                    .unwrap_or(false)
            });
            if !matches {
                return false;
            }
        }

        true
    }
}
