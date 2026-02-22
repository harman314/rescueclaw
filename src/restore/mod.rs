use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

use crate::config::Config;
use crate::validate::{ValidationIssue, Severity};

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
    // Find the backup to restore
    let snapshots = crate::backup::list_snapshots(cfg)?;
    
    if snapshots.is_empty() {
        anyhow::bail!("No backups available. Run `rescueclaw backup` first.");
    }

    let snapshot = if let Some(id) = backup_id {
        snapshots.iter()
            .find(|s| s.id == id)
            .ok_or_else(|| anyhow::anyhow!("Backup '{}' not found. Use `rescueclaw list` to see available backups.", id))?
    } else {
        &snapshots[0] // latest
    };

    println!("🛟 Restoring from backup: {} ({})", snapshot.id, snapshot.size_human);

    // Step 1: Validate backup contents (unless --force)
    if !force {
        println!("  Validating backup...");
        let temp_dir = TempDir::new()?;
        extract_backup_to(&snapshot.path, temp_dir.path(), cfg)?;
        
        // Validate config
        let config_issues = crate::validate::validate_openclaw_config(
            &temp_dir.path().join("config")
        )?;
        
        // Validate workspace
        let workspace_issues = crate::validate::validate_workspace(
            &temp_dir.path().join("workspace")
        )?;
        
        let all_issues: Vec<_> = config_issues.into_iter()
            .chain(workspace_issues.into_iter())
            .collect();
        
        let errors: Vec<_> = all_issues.iter()
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
            println!("  ⚠️  Found {} warning(s) but proceeding...\n", all_issues.len());
        }
    }

    if dry_run {
        println!("  ✓ Dry-run: Backup is valid and would restore successfully");
        println!("\n  Would restore:");
        println!("    - Workspace to: {}", cfg.openclaw.workspace.display());
        println!("    - Config to:    {}", cfg.openclaw.config_path.display());
        return Ok(());
    }

    // Step 2: Extract backup (files only — does NOT restart any gateway)
    println!("  Extracting backup...");
    extract_backup(&snapshot.path, cfg)?;
    println!("  ✓ Files restored to workspace and config directories.");
    println!();
    println!("  ⚠ Gateway was NOT restarted automatically (safety measure).");
    println!("    To restart the correct gateway, run:");
    println!("      openclaw gateway restart");
    println!();
    println!("  Verifying if gateway is responding...");
    let alive = wait_for_agent(5).await;

    if alive {
        println!("  ✓ Gateway is online (files restored, gateway still running).");
    } else {
        println!("  ℹ Gateway not responding. Restart it manually when ready.");
    }

    Ok(())
}

/// Extract backup and optionally analyze incident
pub async fn restore_and_analyze(
    cfg: &Config,
    backup_id: Option<&str>,
    incident: Option<&crate::health::IncidentLog>,
) -> Result<()> {
    // Perform restore
    restore(cfg, backup_id).await?;
    
    // Generate incident analysis if we have incident info
    if let Some(inc) = incident {
        println!("\n  📊 Analyzing incident...");
        match crate::analysis::analyze_incident(cfg).await {
            Ok(analysis) => {
                let backup_id_str = backup_id.unwrap_or("latest");
                let report = crate::analysis::format_incident_report(
                    &analysis,
                    inc,
                    backup_id_str,
                );
                
                // Save report
                let report_path = cfg.backup.path.join(
                    format!("incident-report-{}.md", backup_id_str)
                );
                fs::write(&report_path, &report)?;
                println!("  ✓ Incident report saved to: {}", report_path.display());
                
                // Return report for potential Telegram notification
                return Ok(());
            }
            Err(e) => {
                println!("  ⚠ Analysis failed: {}", e);
            }
        }
    }
    
    Ok(())
}

/// Extract backup to a specific directory (for validation)
fn extract_backup_to(backup_path: &Path, dest_dir: &Path, cfg: &Config) -> Result<()> {
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

/// Extract a backup tarball, restoring workspace and config files
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
            cfg.openclaw.config_path.join("agents/main/sessions").join(relative)
        } else {
            // manifest.json or other metadata — skip
            continue;
        };

        // Ensure parent dir exists
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }

        entry.unpack(&dest)?;
    }

    Ok(())
}

/// Stop OpenClaw gateway
fn stop_openclaw() -> Result<()> {
    let output = Command::new("openclaw")
        .args(["gateway", "stop"])
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            // Try legacy command
            let _ = Command::new("clawdbot")
                .args(["gateway", "stop"])
                .output();
            Ok(()) // Best effort — might already be stopped
        }
        Err(_) => {
            // OpenClaw CLI not in PATH, try direct kill
            let _ = Command::new("pkill").args(["-f", "openclaw"]).output();
            Ok(())
        }
    }
}

/// Start OpenClaw gateway
fn start_openclaw() -> Result<()> {
    let output = Command::new("openclaw")
        .args(["gateway", "start"])
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(_) => {
            // Try legacy command
            Command::new("clawdbot")
                .args(["gateway", "start"])
                .output()
                .context("Failed to start OpenClaw gateway")?;
            Ok(())
        }
        Err(e) => anyhow::bail!("Could not start OpenClaw: {}. Start manually with `openclaw gateway start`", e),
    }
}

/// Wait for the agent to come back online
async fn wait_for_agent(timeout_secs: u64) -> bool {
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);

    while tokio::time::Instant::now() < deadline {
        if let Ok(_) = client
            .get("http://127.0.0.1:7744/api/status")
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
        {
            return true;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    false
}
