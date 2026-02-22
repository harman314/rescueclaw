use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use crate::config::Config;
use crate::validate::Severity;

/// Restore OpenClaw from a backup snapshot
pub async fn restore(cfg: &Config, backup_id: Option<&str>) -> Result<()> {
    restore_with_options(cfg, backup_id, false, false).await
}

/// Restore with validation and dry-run options
pub async fn restore_with_options(
    cfg: &Config,
    backup_id: Option<&str>,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    let snapshots = crate::backup::list_snapshots(cfg)?;

    if snapshots.is_empty() {
        anyhow::bail!("No backups available. Run `rescueclaw backup` first.");
    }

    let snapshot = if let Some(id) = backup_id {
        snapshots.iter().find(|s| s.id == id).ok_or_else(|| {
            anyhow::anyhow!(
                "Backup '{}' not found. Use `rescueclaw list` to see available backups.",
                id
            )
        })?
    } else {
        &snapshots[0]
    };

    println!(
        "🛟 Restoring from backup: {} ({})",
        snapshot.id, snapshot.size_human
    );

    // Step 1: Validate backup contents (unless --force)
    if !force {
        println!("  Validating backup...");
        let temp_dir = TempDir::new()?;
        extract_backup_to(&snapshot.path, temp_dir.path(), cfg)?;

        let config_issues =
            crate::validate::validate_openclaw_config(&temp_dir.path().join("config"))?;
        let workspace_issues =
            crate::validate::validate_workspace(&temp_dir.path().join("workspace"))?;

        let all_issues: Vec<_> = config_issues
            .into_iter()
            .chain(workspace_issues.into_iter())
            .collect();

        let errors: Vec<_> = all_issues
            .iter()
            .filter(|i| matches!(i.severity, Severity::Error))
            .collect();

        if !all_issues.is_empty() {
            println!("\n  Validation issues found:");
            for issue in &all_issues {
                let icon = match issue.severity {
                    Severity::Error => "❌",
                    Severity::Warning => "⚠️",
                };
                println!("    {} {}", icon, issue.message);
            }
            println!();
        }

        if !errors.is_empty() {
            if dry_run {
                println!("  ❌ Restore would fail due to validation errors");
                return Ok(());
            }
            anyhow::bail!(
                "Backup validation failed with {} error(s). Use --force to override.",
                errors.len()
            );
        }

        if !all_issues.is_empty() {
            println!(
                "  ⚠️  Found {} warning(s) but proceeding...\n",
                all_issues.len()
            );
        }
    }

    if dry_run {
        println!("  ✓ Dry-run: Backup is valid and would restore successfully");
        println!("\n  Would restore:");
        println!("    - Workspace to: {}", cfg.openclaw.workspace.display());
        println!("    - Config to:    {}", cfg.openclaw.config_path.display());
        return Ok(());
    }

    // Step 2: Identify the target gateway by port (from OpenClaw config)
    let target_port = read_gateway_port(cfg);
    let gateway_pid = find_gateway_pid(target_port);
    let was_running = gateway_pid.is_some();

    println!(
        "  Target gateway: port {} (PID: {})",
        target_port,
        gateway_pid.map_or("not running".to_string(), |p| p.to_string())
    );

    // Step 3: Stop the specific gateway by PID (only if it was running)
    if let Some(pid) = gateway_pid {
        println!("  Stopping gateway (PID {})...", pid);
        kill_process(pid)?;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    // Step 4: Restore files
    println!("  Extracting backup...");
    extract_backup(&snapshot.path, cfg)?;
    println!("  ✓ Files restored.");

    // Step 5: Only restart if the gateway was running before we stopped it
    if was_running {
        println!("  Restarting gateway on port {}...", target_port);
        start_openclaw_with_config(cfg)?;

        println!("  Verifying gateway is responsive...");
        let alive = wait_for_agent(target_port, 30).await;

        if alive {
            println!("  ✓ Agent restored and online on port {}!", target_port);
        } else {
            println!(
                "  ⚠ Agent started but not responding on port {}.",
                target_port
            );
            println!("    Check manually: openclaw gateway status");
        }
    } else {
        println!(
            "  ℹ No gateway was running on port {} — files restored only.",
            target_port
        );
        println!("    Start it manually when ready: openclaw gateway start");
    }

    Ok(())
}

/// Extract backup and optionally analyze incident
#[allow(dead_code)]
pub async fn restore_and_analyze(
    cfg: &Config,
    backup_id: Option<&str>,
    incident: Option<&crate::health::IncidentLog>,
) -> Result<()> {
    restore(cfg, backup_id).await?;

    if let Some(inc) = incident {
        println!("\n  📊 Analyzing incident...");
        match crate::analysis::analyze_incident(cfg).await {
            Ok(analysis) => {
                let backup_id_str = backup_id.unwrap_or("latest");
                let report = crate::analysis::format_incident_report(&analysis, inc, backup_id_str);
                let report_path = cfg
                    .backup
                    .path
                    .join(format!("incident-report-{}.md", backup_id_str));
                fs::write(&report_path, &report)?;
                println!("  ✓ Incident report saved to: {}", report_path.display());
            }
            Err(e) => {
                println!("  ⚠ Analysis failed: {}", e);
            }
        }
    }

    Ok(())
}

// ─── Gateway targeting ─────────────────────────────────────────────

/// Read the gateway port from the OpenClaw config file
pub fn read_gateway_port(cfg: &Config) -> u16 {
    let config_file = cfg.openclaw.config_path.join("openclaw.json");
    let legacy_file = cfg.openclaw.config_path.join("clawdbot.json");

    let path = if config_file.exists() {
        config_file
    } else {
        legacy_file
    };

    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(port) = json
                .get("gateway")
                .and_then(|g| g.get("port"))
                .and_then(|p| p.as_u64())
            {
                return port as u16;
            }
        }
    }

    // Default OpenClaw port
    7744
}

/// Find the PID of the gateway process listening on a specific port
fn find_gateway_pid(port: u16) -> Option<u32> {
    // Use ss/lsof to find which PID is listening on this port
    let output = Command::new("ss")
        .args(["-tlnp", &format!("sport = :{}", port)])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse ss output for pid=NNNN
    for line in stdout.lines() {
        if let Some(pid_start) = line.find("pid=") {
            let after_pid = &line[pid_start + 4..];
            let pid_str: String = after_pid
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(pid) = pid_str.parse::<u32>() {
                return Some(pid);
            }
        }
    }

    // Fallback: try lsof
    let output = Command::new("lsof")
        .args(["-ti", &format!(":{}", port)])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<u32>().ok())
}

/// Kill a specific process by PID (SIGTERM, then SIGKILL if needed)
fn kill_process(pid: u32) -> Result<()> {
    // Send SIGTERM
    let _ = Command::new("kill").arg(pid.to_string()).output();

    // Wait briefly, check if dead
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Check if still alive
    let check = Command::new("kill").args(["-0", &pid.to_string()]).output();

    if let Ok(o) = check {
        if o.status.success() {
            // Still alive, SIGKILL
            tracing::warn!("Process {} didn't stop with SIGTERM, sending SIGKILL", pid);
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
        }
    }

    Ok(())
}

/// Start OpenClaw gateway using the specific config path
fn start_openclaw_with_config(cfg: &Config) -> Result<()> {
    let config_path = cfg.openclaw.config_path.join("openclaw.json");
    let legacy_path = cfg.openclaw.config_path.join("clawdbot.json");

    // Try openclaw CLI with explicit config
    let result = if config_path.exists() {
        Command::new("openclaw")
            .args([
                "gateway",
                "start",
                "--config",
                &config_path.to_string_lossy(),
            ])
            .output()
    } else if legacy_path.exists() {
        Command::new("clawdbot")
            .args([
                "gateway",
                "start",
                "--config",
                &legacy_path.to_string_lossy(),
            ])
            .output()
    } else {
        // No config file found — try bare start
        Command::new("openclaw").args(["gateway", "start"]).output()
    };

    match result {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // Systemd-managed services may not support --config, try plain restart
            tracing::info!(
                "Config-targeted start failed ({}), trying plain restart",
                stderr.trim()
            );
            let _ = Command::new("systemctl")
                .args(["--user", "restart", "openclaw-gateway"])
                .output();
            Ok(())
        }
        Err(e) => {
            anyhow::bail!(
                "Could not start gateway: {}. Start manually: openclaw gateway start",
                e
            );
        }
    }
}

// ─── Backup extraction ─────────────────────────────────────────────

/// Extract backup to a specific directory (for validation / dry-run)
fn extract_backup_to(backup_path: &Path, dest_dir: &Path, _cfg: &Config) -> Result<()> {
    let tar_file = fs::File::open(backup_path)?;
    let decoder = GzDecoder::new(tar_file);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let path_str = path.to_string_lossy();

        let dest = if path_str.starts_with("workspace/")
            || path_str.starts_with("config/")
            || path_str.starts_with("sessions/")
        {
            dest_dir.join(&*path)
        } else {
            continue;
        };

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&dest)?;
    }

    Ok(())
}

/// Extract a backup tarball to the real workspace and config directories
fn extract_backup(backup_path: &Path, cfg: &Config) -> Result<()> {
    let tar_file = fs::File::open(backup_path)
        .with_context(|| format!("opening backup: {}", backup_path.display()))?;
    let decoder = GzDecoder::new(tar_file);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let path_str = path.to_string_lossy();

        let dest = if path_str.starts_with("workspace/") {
            let relative = path_str.strip_prefix("workspace/").unwrap();
            cfg.openclaw.workspace.join(relative)
        } else if path_str.starts_with("config/") {
            let relative = path_str.strip_prefix("config/").unwrap();
            cfg.openclaw.config_path.join(relative)
        } else if path_str.starts_with("sessions/") {
            let relative = path_str.strip_prefix("sessions/").unwrap();
            cfg.openclaw
                .config_path
                .join("agents/main/sessions")
                .join(relative)
        } else {
            continue;
        };

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&dest)?;
    }

    Ok(())
}

// ─── Health check (port-aware) ─────────────────────────────────────

/// Wait for the agent to come back online on the correct port
async fn wait_for_agent(port: u16, timeout_secs: u64) -> bool {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/status", port);
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);

    while tokio::time::Instant::now() < deadline {
        if client
            .get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    false
}
