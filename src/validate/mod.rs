use anyhow::Result;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub message: String,
}

impl ValidationIssue {
    fn error(msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: msg.into(),
        }
    }

    fn warning(msg: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: msg.into(),
        }
    }
}

/// Validate OpenClaw config file
pub fn validate_openclaw_config(config_path: &Path) -> Result<Vec<ValidationIssue>> {
    let mut issues = Vec::new();

    // Find the actual config file (openclaw.json or clawdbot.json)
    let config_file = if config_path.join("openclaw.json").exists() {
        config_path.join("openclaw.json")
    } else if config_path.join("clawdbot.json").exists() {
        config_path.join("clawdbot.json")
    } else if config_path.is_file() && config_path.exists() {
        config_path.to_path_buf()
    } else {
        issues.push(ValidationIssue::error("OpenClaw config file not found"));
        return Ok(issues);
    };

    // Parse config
    let content = match std::fs::read_to_string(&config_file) {
        Ok(c) => c,
        Err(e) => {
            issues.push(ValidationIssue::error(format!(
                "Failed to read config: {}",
                e
            )));
            return Ok(issues);
        }
    };

    let config: Value = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            issues.push(ValidationIssue::error(format!("Invalid JSON: {}", e)));
            return Ok(issues);
        }
    };

    // Check for required fields
    if let Some(default_model) = config.get("defaultModel") {
        if default_model.as_str().is_none_or(|s| s.is_empty()) {
            issues.push(ValidationIssue::error("defaultModel is empty"));
        }
    } else {
        issues.push(ValidationIssue::error("Missing defaultModel field"));
    }

    // Check providers
    if let Some(providers) = config.get("providers") {
        if let Some(providers_obj) = providers.as_object() {
            if providers_obj.is_empty() {
                issues.push(ValidationIssue::error("No providers configured"));
            } else {
                // Validate each provider
                for (name, provider) in providers_obj {
                    if let Some(api_key) = provider.get("apiKey") {
                        if let Some(key_str) = api_key.as_str() {
                            if key_str.is_empty() {
                                issues.push(ValidationIssue::error(format!(
                                    "Provider '{}' has empty apiKey",
                                    name
                                )));
                            } else if is_placeholder_key(key_str) {
                                issues.push(ValidationIssue::warning(format!(
                                    "Provider '{}' appears to have placeholder apiKey",
                                    name
                                )));
                            }
                        }
                    } else {
                        issues.push(ValidationIssue::error(format!(
                            "Provider '{}' missing apiKey",
                            name
                        )));
                    }
                }
            }
        } else {
            issues.push(ValidationIssue::error("providers must be an object"));
        }
    } else {
        issues.push(ValidationIssue::error("Missing providers field"));
    }

    // Check gateway config
    if config.get("gateway").is_none() {
        issues.push(ValidationIssue::warning("Missing gateway configuration"));
    } else if let Some(gateway) = config.get("gateway") {
        if let Some(port) = gateway.get("port") {
            if let Some(port_num) = port.as_u64() {
                if port_num == 0 || port_num > 65535 {
                    issues.push(ValidationIssue::error(format!(
                        "Invalid gateway port: {}",
                        port_num
                    )));
                }
            }
        }
    }

    Ok(issues)
}

/// Validate OpenClaw workspace
pub fn validate_workspace(workspace_path: &Path) -> Result<Vec<ValidationIssue>> {
    let mut issues = Vec::new();

    if !workspace_path.exists() {
        issues.push(ValidationIssue::error("Workspace path does not exist"));
        return Ok(issues);
    }

    if !workspace_path.is_dir() {
        issues.push(ValidationIssue::error("Workspace path is not a directory"));
        return Ok(issues);
    }

    // Check for critical files
    if !workspace_path.join("SOUL.md").exists() {
        issues.push(ValidationIssue::error(
            "Missing SOUL.md - agent identity file",
        ));
    }

    if !workspace_path.join("AGENTS.md").exists() {
        issues.push(ValidationIssue::warning("Missing AGENTS.md"));
    }

    // Check memory directory
    if !workspace_path.join("memory").exists() {
        issues.push(ValidationIssue::warning("Missing memory/ directory"));
    } else if !workspace_path.join("memory").is_dir() {
        issues.push(ValidationIssue::error(
            "memory exists but is not a directory",
        ));
    }

    // Warn if workspace seems empty
    if let Ok(entries) = std::fs::read_dir(workspace_path) {
        let count = entries.count();
        if count < 3 {
            issues.push(ValidationIssue::warning(format!(
                "Workspace seems sparse (only {} items)",
                count
            )));
        }
    }

    Ok(issues)
}

/// Check if an API key looks like a placeholder
fn is_placeholder_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    key_lower.contains("your_key")
        || key_lower.contains("your-key")
        || key_lower.contains("placeholder")
        || key_lower.contains("xxx")
        || key_lower.contains("replace")
        || key == "sk-"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder_detection() {
        assert!(is_placeholder_key("YOUR_KEY_HERE"));
        assert!(is_placeholder_key("sk-xxx"));
        assert!(is_placeholder_key("placeholder-key"));
        assert!(!is_placeholder_key("sk-1234567890abcdef"));
    }

    #[test]
    fn test_validation_issue_creation() {
        let err = ValidationIssue::error("test error");
        assert_eq!(err.severity, Severity::Error);
        assert_eq!(err.message, "test error");

        let warn = ValidationIssue::warning("test warning");
        assert_eq!(warn.severity, Severity::Warning);
    }
}
