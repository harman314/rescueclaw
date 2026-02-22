use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    pub agent_online: bool,
    pub agent_uptime: Option<String>,
    pub watchdog_pid: u32,
    pub watchdog_memory_mb: f64,
    pub last_backup: Option<String>,
    pub backup_count: usize,
    pub consecutive_failures: u32,
    pub skill_installed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IncidentLog {
    pub timestamp: String,
    pub cause: String,
    pub recovery: String,
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "🛟 RescueClaw Status\n")?;
        writeln!(f, "Agent:       {} {}",
            if self.agent_online { "✅" } else { "❌" },
            if self.agent_online { 
                format!("Online{}", self.agent_uptime.as_deref().map_or(String::new(), |u| format!(" (uptime: {})", u)))
            } else { 
                "OFFLINE".to_string() 
            }
        )?;
        writeln!(f, "Watchdog:    ✅ Running (PID {}, {:.1}MB RAM)", self.watchdog_pid, self.watchdog_memory_mb)?;
        writeln!(f, "Last backup: {}", self.last_backup.as_deref().unwrap_or("never"))?;
        writeln!(f, "Backups:     {} snapshots stored", self.backup_count)?;
        writeln!(f, "Health:      {} consecutive check failures", self.consecutive_failures)?;
        writeln!(f, "Skill:       {}", if self.skill_installed { "✅ Installed" } else { "⚠️  Not installed" })?;
        Ok(())
    }
}

/// Check current status of the agent and watchdog
pub async fn check_status(cfg: &Config) -> Result<HealthStatus> {
    let agent_online = check_agent_alive(cfg).await;
    let backup_count = crate::backup::list_snapshots(cfg)?.len();
    let last_backup = crate::backup::list_snapshots(cfg)?
        .first()
        .map(|s| s.timestamp.clone());

    // Check if rescueclaw skill is installed in OpenClaw
    let skill_installed = cfg.openclaw.workspace
        .join("skills/rescueclaw-skill")
        .exists()
        || check_skill_via_clawhub(cfg);

    Ok(HealthStatus {
        agent_online,
        agent_uptime: None, // TODO: parse from OpenClaw status
        watchdog_pid: std::process::id(),
        watchdog_memory_mb: get_memory_usage_mb(),
        last_backup,
        backup_count,
        consecutive_failures: 0, // TODO: track in state file
        skill_installed,
    })
}

/// Check if OpenClaw gateway is responding
async fn check_agent_alive(cfg: &Config) -> bool {
    // Try to hit the OpenClaw gateway status endpoint
    let client = reqwest::Client::new();
    let result = client
        .get("http://127.0.0.1:7744/api/status")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    result.is_ok()
}

/// Check if rescueclaw skill is installed via clawhub
fn check_skill_via_clawhub(_cfg: &Config) -> bool {
    // TODO: check clawhub installed skills list
    false
}

/// Get current process memory usage in MB
fn get_memory_usage_mb() -> f64 {
    // Read from /proc/self/status on Linux
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<f64>() {
                        return kb / 1024.0;
                    }
                }
            }
        }
    }
    0.0
}

/// Continuous health monitoring loop
pub async fn health_loop(cfg: &Config) -> Result<()> {
    let interval = parse_health_interval(&cfg.health.check_interval)?;
    let mut consecutive_failures: u32 = 0;
    let incidents_path = cfg.backup.path.join("incidents.jsonl");

    loop {
        tokio::time::sleep(interval).await;

        let alive = check_agent_alive(cfg).await;

        if alive {
            if consecutive_failures > 0 {
                tracing::info!("Agent recovered after {} failed checks", consecutive_failures);
            }
            consecutive_failures = 0;
        } else {
            consecutive_failures += 1;
            tracing::warn!("Agent unresponsive (check #{}/{})", 
                consecutive_failures, cfg.health.unhealthy_threshold);

            // Log the incident
            let incident = IncidentLog {
                timestamp: Utc::now().to_rfc3339(),
                cause: format!("Agent unresponsive (check #{})", consecutive_failures),
                recovery: "pending".to_string(),
            };
            if let Ok(line) = serde_json::to_string(&incident) {
                let _ = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&incidents_path)
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, "{}", line)
                    });
            }

            // Auto-restore if enabled and threshold reached
            if cfg.health.auto_restore && consecutive_failures >= cfg.health.unhealthy_threshold {
                tracing::error!("Threshold reached! Initiating auto-restore...");
                if let Err(e) = crate::restore::restore(cfg, None).await {
                    tracing::error!("Auto-restore failed: {}", e);
                } else {
                    consecutive_failures = 0;
                }
            }
        }
    }
}

/// Read recent incident logs
pub fn recent_incidents(cfg: &Config, n: usize) -> Result<Vec<IncidentLog>> {
    let incidents_path = cfg.backup.path.join("incidents.jsonl");
    if !incidents_path.exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(&incidents_path)?;
    let incidents: Vec<IncidentLog> = content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    Ok(incidents.into_iter().rev().take(n).collect())
}

fn parse_health_interval(s: &str) -> Result<tokio::time::Duration> {
    let s = s.trim();
    if let Some(m) = s.strip_suffix('m') {
        Ok(tokio::time::Duration::from_secs(m.parse::<u64>()? * 60))
    } else if let Some(s_val) = s.strip_suffix('s') {
        Ok(tokio::time::Duration::from_secs(s_val.parse::<u64>()?))
    } else {
        anyhow::bail!("Invalid interval: {}", s)
    }
}
