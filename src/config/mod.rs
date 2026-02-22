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
    use std::io::{self, Write};
    
    println!("🛟 RescueClaw Setup");
    println!("━━━━━━━━━━━━━━━━━━\n");
    
    // Step 1: Detect OpenClaw
    println!("Step 1/6: Detect OpenClaw");
    let workspace = detect_openclaw_workspace()?;
    println!("  ✓ Workspace: {}", workspace.display());
    
    let config_path = detect_openclaw_config()?;
    println!("  ✓ Config:    {}", config_path.display());
    
    // Check if gateway is running
    let gateway_running = check_gateway_running().await;
    if gateway_running {
        println!("  ✓ Gateway:   Running");
    } else {
        println!("  ⚠ Gateway:   Not responding on :7744");
    }
    
    // Validate OpenClaw config
    let oc_config = OpenClawConfig {
        workspace: workspace.clone(),
        config_path: config_path.clone(),
    };
    let temp_cfg = Config {
        openclaw: oc_config.clone(),
        ..Default::default()
    };
    
    match temp_cfg.read_openclaw_providers() {
        Ok(_) => println!("  ✓ Config:    Valid"),
        Err(e) => println!("  ⚠ Config:    {}", e),
    }
    
    println!();
    
    // Step 2: Telegram Bot
    println!("Step 2/6: Telegram Bot");
    println!("  1. Open @BotFather on Telegram");
    println!("  2. Send /newbot and name it (e.g., 'MyRescueClaw')");
    println!("  3. Copy the bot token\n");
    
    let token = loop {
        let input = prompt("Bot token: ", "")?;
        if input.is_empty() {
            continue;
        }
        
        // Validate format (digits:alphanumeric)
        if !input.contains(':') || input.len() < 20 {
            println!("  ❌ Invalid format. Expected format: 123456:ABC-DEF...");
            continue;
        }
        
        // Test token
        print!("  Testing token...");
        io::stdout().flush()?;
        match validate_telegram_token(&input).await {
            Ok(bot_name) => {
                println!(" ✓ Connected to @{}", bot_name);
                break input;
            }
            Err(e) => {
                println!(" ❌ Failed: {}", e);
                continue;
            }
        }
    };
    
    println!("\n  Now send /start to your bot in Telegram.");
    println!("  Then get your user ID from @userinfobot (send any message to it).\n");
    
    let user_id: i64 = loop {
        let input = prompt("Your Telegram user ID: ", "")?;
        match input.parse() {
            Ok(id) => break id,
            Err(_) => {
                println!("  ❌ Must be a number");
                continue;
            }
        }
    };
    
    println!();
    
    // Step 3: Backup Settings
    println!("Step 3/6: Backup Settings");
    let backup_interval = prompt("Backup interval [6h]: ", "6h")?;
    let max_snapshots: usize = prompt("Max snapshots to keep [10]: ", "10")?.parse()
        .unwrap_or(10);
    let backup_path = PathBuf::from(prompt("Backup path [/var/rescueclaw/backups]: ", 
        "/var/rescueclaw/backups")?);
    let include_sessions = prompt_yn("Include session files? [n]: ", false)?;
    
    // Validate backup path
    if let Err(e) = std::fs::create_dir_all(&backup_path) {
        println!("  ⚠ Warning: Could not create backup dir: {}", e);
    } else {
        println!("  ✓ Backup directory ready");
    }
    
    println!();
    
    // Step 4: Health Check Settings
    println!("Step 4/6: Health Check Settings");
    let check_interval = prompt("Health check interval [5m]: ", "5m")?;
    let unhealthy_threshold: u32 = prompt("Failures before auto-restore [3]: ", "3")?.parse()
        .unwrap_or(3);
    let auto_restore = prompt_yn("Enable auto-restore? [y]: ", true)?;
    
    println!();
    
    // Step 5: Write Config
    println!("Step 5/6: Write Config");
    let config = Config {
        backup: BackupConfig {
            interval: backup_interval,
            max_snapshots,
            path: backup_path,
            include_sessions,
        },
        health: HealthConfig {
            check_interval,
            unhealthy_threshold,
            auto_restore,
            auto_restore_cooldown: Some("1h".to_string()),
        },
        telegram: TelegramConfig {
            token,
            allowed_users: vec![user_id],
        },
        openclaw: oc_config,
    };
    
    let config_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
        .join(".config/rescueclaw");
    std::fs::create_dir_all(&config_dir)?;
    
    let config_file = config_dir.join("rescueclaw.json");
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(&config_file, json)?;
    
    println!("  ✓ Config written to {}", config_file.display());
    println!();
    
    // Step 6: First Backup & Service Install
    println!("Step 6/6: First Backup & Service Install");
    
    print!("  Taking first backup...");
    io::stdout().flush()?;
    match crate::backup::take_snapshot(&config) {
        Ok(snap) => println!(" ✓ {}", snap.id),
        Err(e) => println!(" ❌ {}", e),
    }
    
    println!();
    if prompt_yn("Install systemd service? [y]: ", true)? {
        install_systemd_service(&config)?;
    } else {
        println!("  Skipped. Run 'sudo rescueclaw install' later to install the service.");
    }
    
    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Setup Complete!");
    println!();
    println!("Your AI agent now has a safety net.");
    println!("RescueClaw will:");
    println!("  • Take backups every {}", config.backup.interval);
    println!("  • Check health every {}", config.health.check_interval);
    if config.health.auto_restore {
        println!("  • Auto-restore after {} consecutive failures", config.health.unhealthy_threshold);
    }
    println!();
    println!("Start the daemon:  sudo systemctl start rescueclaw");
    println!("View status:       rescueclaw status");
    println!("List backups:      rescueclaw list");
    println!();
    
    Ok(())
}

/// Helper: prompt for input with default
fn prompt(question: &str, default: &str) -> Result<String> {
    use std::io::{self, Write};
    print!("  {}", question);
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    
    if input.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(input.to_string())
    }
}

/// Helper: prompt for yes/no
fn prompt_yn(question: &str, default: bool) -> Result<bool> {
    let input = prompt(question, if default { "y" } else { "n" })?;
    Ok(matches!(input.to_lowercase().as_str(), "y" | "yes"))
}

/// Validate Telegram token by calling getMe API
async fn validate_telegram_token(token: &str) -> Result<String> {
    let url = format!("https://api.telegram.org/bot{}/getMe", token);
    let resp = reqwest::get(&url).await?;
    
    if !resp.status().is_success() {
        anyhow::bail!("Invalid token or network error");
    }
    
    let json: serde_json::Value = resp.json().await?;
    let bot_name = json["result"]["username"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    
    Ok(bot_name)
}

/// Check if OpenClaw gateway is running
async fn check_gateway_running() -> bool {
    reqwest::get("http://127.0.0.1:7744/api/status")
        .await
        .is_ok()
}

/// Detect OpenClaw config directory
fn detect_openclaw_config() -> Result<PathBuf> {
    let candidates = vec![
        dirs::home_dir().map(|h| h.join(".openclaw")),
        dirs::home_dir().map(|h| h.join(".clawdbot")),
    ];
    
    for candidate in candidates.into_iter().flatten() {
        if candidate.join("openclaw.json").exists() || candidate.join("clawdbot.json").exists() {
            return Ok(candidate);
        }
    }
    
    anyhow::bail!("Could not find OpenClaw config directory (~/.openclaw or ~/.clawdbot)")
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

/// Generate systemd service file content
fn generate_service_file(cfg: &Config) -> String {
    let binary_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "/usr/local/bin/rescueclaw".to_string());
    
    format!(r#"[Unit]
Description=RescueClaw - AI Agent Watchdog
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={} start
Restart=on-failure
RestartSec=10
Environment=RUST_LOG=info

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths={} {} {}
ProtectHome=read-only

[Install]
WantedBy=multi-user.target
"#,
        binary_path,
        cfg.backup.path.display(),
        cfg.openclaw.workspace.display(),
        cfg.openclaw.config_path.display()
    )
}

/// Install systemd service
pub fn install_systemd_service(cfg: &Config) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    
    println!("  Installing systemd service...");
    
    let service_content = generate_service_file(cfg);
    
    // Write service file using sudo tee
    let mut child = Command::new("sudo")
        .args(["tee", "/etc/systemd/system/rescueclaw.service"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(service_content.as_bytes())?;
    }
    
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("Failed to write service file");
    }
    
    // Reload systemd
    Command::new("sudo")
        .args(["systemctl", "daemon-reload"])
        .status()?;
    
    // Enable service
    Command::new("sudo")
        .args(["systemctl", "enable", "rescueclaw"])
        .status()?;
    
    // Start service
    let start_status = Command::new("sudo")
        .args(["systemctl", "start", "rescueclaw"])
        .status()?;
    
    if start_status.success() {
        println!("  ✓ Service installed and started");
        println!("  View logs: sudo journalctl -u rescueclaw -f");
    } else {
        println!("  ⚠ Service installed but failed to start");
        println!("  Check: sudo systemctl status rescueclaw");
    }
    
    Ok(())
}

/// Uninstall the watchdog service
pub fn uninstall() -> Result<()> {
    use std::process::Command;
    
    println!("🛟 Uninstalling RescueClaw...");
    
    // Stop service
    let _ = Command::new("sudo")
        .args(["systemctl", "stop", "rescueclaw"])
        .status();
    
    // Disable service
    let _ = Command::new("sudo")
        .args(["systemctl", "disable", "rescueclaw"])
        .status();
    
    // Remove service file
    let _ = Command::new("sudo")
        .args(["rm", "/etc/systemd/system/rescueclaw.service"])
        .status();
    
    // Reload systemd
    let _ = Command::new("sudo")
        .args(["systemctl", "daemon-reload"])
        .status();
    
    println!("  ✓ Service uninstalled");
    println!("  Backups preserved at /var/rescueclaw/backups/");
    println!("  Config preserved at ~/.config/rescueclaw/");
    Ok(())
}
