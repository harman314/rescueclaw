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

    // Check for primary model (agents.defaults.model.primary or agents.defaults.model as string)
    let has_model = config
        .pointer("/agents/defaults/model/primary")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
        || config
            .pointer("/agents/defaults/model")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());

    if !has_model {
        issues.push(ValidationIssue::warning(
            "No primary model configured (agents.defaults.model.primary)",
        ));
    }

    // Check model providers (models.providers)
    if let Some(providers) = config.pointer("/models/providers") {
        if let Some(providers_obj) = providers.as_object() {
            if providers_obj.is_empty() {
                issues.push(ValidationIssue::warning("No model providers configured"));
            } else {
                // Validate each provider has a valid baseUrl
                for (name, provider) in providers_obj {
                    if let Some(base_url) = provider.get("baseUrl").and_then(|v| v.as_str()) {
                        if base_url.is_empty() {
                            issues.push(ValidationIssue::error(format!(
                                "Provider '{}' has empty baseUrl",
                                name
                            )));
                        } else if !base_url.starts_with("https://")
                            && !base_url.starts_with("http://")
                        {
                            issues.push(ValidationIssue::error(format!(
                                "Provider '{}' has invalid baseUrl: {}",
                                name, base_url
                            )));
                        }
                    }

                    // Check inline apiKey if present (some providers use it, others use auth profiles)
                    if let Some(api_key) = provider.get("apiKey").and_then(|v| v.as_str()) {
                        if api_key.is_empty() {
                            issues.push(ValidationIssue::warning(format!(
                                "Provider '{}' has empty apiKey",
                                name
                            )));
                        } else if is_placeholder_key(api_key) {
                            issues.push(ValidationIssue::warning(format!(
                                "Provider '{}' appears to have placeholder apiKey",
                                name
                            )));
                        }
                    }
                    // Note: Anthropic uses auth profiles (auth.profiles), not inline apiKey — that's fine

                    // Check models array exists and is non-empty
                    if let Some(models) = provider.get("models") {
                        if let Some(arr) = models.as_array() {
                            if arr.is_empty() {
                                issues.push(ValidationIssue::warning(format!(
                                    "Provider '{}' has empty models list",
                                    name
                                )));
                            }
                        }
                    }
                }
            }
        }
    } else {
        issues.push(ValidationIssue::warning(
            "No model providers section (models.providers) — using built-in defaults",
        ));
    }

    // Check auth profiles exist (OpenClaw uses auth.profiles for API keys)
    if let Some(auth) = config.get("auth") {
        if let Some(profiles) = auth.get("profiles").and_then(|v| v.as_object()) {
            if profiles.is_empty() {
                issues.push(ValidationIssue::warning("No auth profiles configured"));
            }
        }
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
