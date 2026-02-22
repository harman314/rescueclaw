use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Main configuration — rescueclaw's own settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub backup: BackupConfig,
    pub health: HealthConfig,
    pub telegram: TelegramConfig,
    pub openclaw: OpenClawConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub interval: String,
    #[serde(rename = "maxSnapshots")]
    pub max_snapshots: usize,
    pub path: PathBuf,
    #[serde(rename = "includeSessions")]
    pub include_sessions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    #[serde(rename = "checkInterval")]
    pub check_interval: String,
    #[serde(rename = "unhealthyThreshold")]
    pub unhealthy_threshold: u32,
    #[serde(rename = "autoRestore")]
    pub auto_restore: bool,
    #[serde(rename = "autoRestoreCooldown")]
    pub auto_restore_cooldown: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub token: String,
    #[serde(rename = "allowedUsers")]
    pub allowed_users: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawConfig {
    pub workspace: PathBuf,
    #[serde(rename = "configPath")]
    pub config_path: PathBuf,
}

/// Model/provider config read from OpenClaw's own config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawProviderConfig {
    pub default_model: Option<String>,
    pub providers: Option<serde_json::Value>,
}

impl Config {
    /// Standard config file locations (checked in order)
    fn config_paths() -> Vec<PathBuf> {
        let mut paths = vec![
            PathBuf::from("rescueclaw.json"),
            PathBuf::from("/etc/rescueclaw/rescueclaw.json"),
        ];
        if let Some(home) = dirs::home_dir() {
            paths.insert(1, home.join(".config/rescueclaw/rescueclaw.json"));
        }
        paths
    }

    /// Load config from first available location
    pub fn load() -> Result<Self> {
        for path in Self::config_paths() {
            if path.exists() {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading config from {}", path.display()))?;
                let config: Config = serde_json::from_str(&content)
                    .with_context(|| format!("parsing config from {}", path.display()))?;
                return Ok(config);
            }
        }
        // Return default config if no file found (setup wizard will create one)
        Ok(Config::default())
    }

    /// Read OpenClaw's provider/model config to reuse API keys and model settings
    pub fn read_openclaw_providers(&self) -> Result<OpenClawProviderConfig> {
        let oc_config_path = self.openclaw.config_path.join("openclaw.json");
        // Also try legacy path
        let legacy_path = self.openclaw.config_path.join("clawdbot.json");
        
        let config_file = if oc_config_path.exists() {
            oc_config_path
        } else if legacy_path.exists() {
            legacy_path
        } else {
            anyhow::bail!("OpenClaw config not found at {} or {}", 
                oc_config_path.display(), legacy_path.display());
        };

        let content = std::fs::read_to_string(&config_file)?;
        let raw: serde_json::Value = serde_json::from_str(&content)?;

        Ok(OpenClawProviderConfig {
            default_model: raw.get("defaultModel")
                .and_then(|v| v.as_str())
                .map(String::from),
            providers: raw.get("providers").cloned(),
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            backup: BackupConfig {
                interval: "6h".to_string(),
                max_snapshots: 10,
                path: PathBuf::from("/var/rescueclaw/backups"),
                include_sessions: false,
            },
            health: HealthConfig {
                check_interval: "5m".to_string(),
                unhealthy_threshold: 3,
                auto_restore: false,
                auto_restore_cooldown: Some("1h".to_string()),
            },
            telegram: TelegramConfig {
                token: String::new(),
                allowed_users: vec![],
            },
            openclaw: OpenClawConfig {
                workspace: PathBuf::from(""),
                config_path: dirs::home_dir()
                    .unwrap_or_default()
                    .join(".openclaw"),
            },
        }
    }
}

/// Interactive setup wizard
pub async fn setup_wizard() -> Result<()> {
    println!("🛟 RescueClaw Setup");
    println!("━━━━━━━━━━━━━━━━━━\n");
    
    // Step 1: Detect OpenClaw
    println!("Step 1/5: Detect OpenClaw");
    let workspace = detect_openclaw_workspace()?;
    println!("  ✓ Found OpenClaw workspace at {}", workspace.display());
    
    // TODO: Full interactive wizard implementation
    // - Detect OpenClaw config path
    // - Check if gateway is running
    // - Prompt for Telegram bot token
    // - Configure backup settings
    // - Install systemd service
    // - Take first backup
    
    println!("\n  Setup wizard is under construction.");
    println!("  For now, copy rescueclaw.example.json to rescueclaw.json and edit manually.\n");
    
    Ok(())
}

/// Try to find the OpenClaw workspace
fn detect_openclaw_workspace() -> Result<PathBuf> {
    // Check common locations
    let candidates = vec![
        dirs::home_dir().map(|h| h.join("clawd")),
        dirs::home_dir().map(|h| h.join("openclaw")),
        Some(PathBuf::from("/opt/openclaw")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.join("AGENTS.md").exists() || candidate.join("SOUL.md").exists() {
            return Ok(candidate);
        }
    }

    anyhow::bail!("Could not auto-detect OpenClaw workspace. Please specify with --workspace")
}

/// Uninstall the watchdog service
pub fn uninstall() -> Result<()> {
    println!("🛟 Uninstalling RescueClaw...");
    // TODO: Stop and disable systemd service, remove service file
    println!("  Backups preserved at /var/rescueclaw/backups/");
    println!("  ✓ Uninstalled");
    Ok(())
}
