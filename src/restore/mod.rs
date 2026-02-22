use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::config::Config;

/// Restore OpenClaw from a backup snapshot
pub async fn restore(cfg: &Config, backup_id: Option<&str>) -> Result<()> {
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

    // Step 1: Stop OpenClaw gateway
    println!("  Stopping OpenClaw gateway...");
    stop_openclaw()?;

    // Step 2: Extract backup
    println!("  Extracting backup...");
    extract_backup(&snapshot.path, cfg)?;

    // Step 3: Restart OpenClaw gateway
    println!("  Restarting OpenClaw gateway...");
    start_openclaw()?;

    // Step 4: Verify it's alive
    println!("  Verifying agent is responsive...");
    let alive = wait_for_agent(30).await;

    if alive {
        println!("  ✓ Agent restored and online!");
    } else {
        println!("  ⚠ Agent started but not yet responding. Check manually.");
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
