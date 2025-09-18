use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub logging: LoggingConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub default: bool,
    pub rules: Vec<LoggingRule>,
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
        let new_config = Config::from_file("config.yaml")?;
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
        Ok(serde_yaml::from_reader(f)?)
    }

    pub fn should_log_request(&self, req: &axum::extract::Request) -> Option<&CaptureConfig> {
        for rule in &self.logging.rules {
            if self.matches_rule(req, &rule.match_conditions) {
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

    fn matches_rule(&self, req: &axum::extract::Request, conditions: &MatchConditions) -> bool {
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

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_yaml() {
        let config = Config::from_file("config.yaml").unwrap();

        // Verify logging rules
        assert_eq!(config.logging.default, false);
        assert_eq!(config.logging.rules.len(), 2);

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
}
