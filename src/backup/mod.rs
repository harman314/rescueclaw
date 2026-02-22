use anyhow::Result;
use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;

/// A backup snapshot
#[derive(Debug)]
pub struct Snapshot {
    pub id: String,
    pub filename: String,
    pub path: PathBuf,
    pub timestamp: String,
    pub size_human: String,
    pub verified: bool,
    pub file_count: usize,
}

/// Files/dirs to always back up (relative to workspace)
const CORE_FILES: &[&str] = &[
    "SOUL.md",
    "IDENTITY.md",
    "AGENTS.md",
    "USER.md",
    "MEMORY.md",
    "TOOLS.md",
    "HEARTBEAT.md",
    "TODO.md",
    "memory",
    "scripts",
];

/// OpenClaw config files to back up (relative to config path)
const CONFIG_FILES: &[&str] = &[
    "openclaw.json",
    "clawdbot.json",   // legacy
    "agents",           // agent configs
];

/// Take a backup snapshot of the OpenClaw workspace + config
pub fn take_snapshot(cfg: &Config) -> Result<Snapshot> {
    let now = Utc::now();
    let id = format!("{}", now.format("%Y%m%d-%H%M%S"));
    let filename = format!("backup-{}.tar.gz", id);
    let backup_path = cfg.backup.path.join(&filename);

    // Ensure backup directory exists
    fs::create_dir_all(&cfg.backup.path)?;

    // Create tarball
    let tar_file = fs::File::create(&backup_path)?;
    let enc = GzEncoder::new(tar_file, Compression::default());
    let mut tar = tar::Builder::new(enc);

    let mut file_count = 0;

    // Add workspace files
    for entry in CORE_FILES {
        let full_path = cfg.openclaw.workspace.join(entry);
        if full_path.exists() {
            if full_path.is_dir() {
                tar.append_dir_all(format!("workspace/{}", entry), &full_path)?;
            } else {
                tar.append_path_with_name(&full_path, format!("workspace/{}", entry))?;
            }
            file_count += 1;
        }
    }

    // Add OpenClaw config files
    for entry in CONFIG_FILES {
        let full_path = cfg.openclaw.config_path.join(entry);
        if full_path.exists() {
            if full_path.is_dir() {
                tar.append_dir_all(format!("config/{}", entry), &full_path)?;
            } else {
                tar.append_path_with_name(&full_path, format!("config/{}", entry))?;
            }
            file_count += 1;
        }
    }

    // Optionally include sessions
    if cfg.backup.include_sessions {
        let sessions_path = cfg.openclaw.config_path.join("agents/main/sessions");
        if sessions_path.exists() {
            tar.append_dir_all("sessions", &sessions_path)?;
            file_count += 1;
        }
    }

    // Add manifest
    let manifest = serde_json::json!({
        "id": id,
        "timestamp": now.to_rfc3339(),
        "file_count": file_count,
        "workspace": cfg.openclaw.workspace,
        "version": env!("CARGO_PKG_VERSION"),
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let mut header = tar::Header::new_gnu();
    header.set_size(manifest_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "manifest.json", &manifest_bytes[..])?;

    tar.finish()?;

    // Get file size
    let metadata = fs::metadata(&backup_path)?;
    let size = metadata.len();
    let size_human = human_size(size);

    // Prune old backups
    prune_old_snapshots(cfg)?;

    Ok(Snapshot {
        id,
        filename,
        path: backup_path,
        timestamp: now.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        size_human,
        verified: true,
        file_count,
    })
}

/// List all available backup snapshots
pub fn list_snapshots(cfg: &Config) -> Result<Vec<Snapshot>> {
    let mut snapshots = Vec::new();

    if !cfg.backup.path.exists() {
        return Ok(snapshots);
    }

    let mut entries: Vec<_> = fs::read_dir(&cfg.backup.path)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "gz"))
        .collect();

    entries.sort_by_key(|e| e.file_name());
    entries.reverse(); // newest first

    for entry in entries {
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();
        let id = filename
            .strip_prefix("backup-")
            .unwrap_or(&filename)
            .strip_suffix(".tar.gz")
            .unwrap_or(&filename)
            .to_string();
        let metadata = fs::metadata(&path)?;

        snapshots.push(Snapshot {
            id,
            filename,
            path,
            timestamp: chrono::DateTime::<chrono::Utc>::from(metadata.modified()?)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            size_human: human_size(metadata.len()),
            verified: true, // TODO: actual verification
            file_count: 0,  // TODO: read from manifest
        });
    }

    Ok(snapshots)
}

/// Remove old snapshots beyond max_snapshots
fn prune_old_snapshots(cfg: &Config) -> Result<()> {
    let snapshots = list_snapshots(cfg)?;
    if snapshots.len() > cfg.backup.max_snapshots {
        for old in &snapshots[cfg.backup.max_snapshots..] {
            fs::remove_file(&old.path)?;
            tracing::info!("Pruned old backup: {}", old.filename);
        }
    }
    Ok(())
}

/// Scheduled backup loop
pub async fn backup_loop(cfg: &Config) -> Result<()> {
    let interval = parse_duration(&cfg.backup.interval)?;
    loop {
        tokio::time::sleep(interval).await;
        match take_snapshot(cfg) {
            Ok(snap) => tracing::info!("Scheduled backup: {} ({})", snap.filename, snap.size_human),
            Err(e) => tracing::error!("Backup failed: {}", e),
        }
    }
}

fn parse_duration(s: &str) -> Result<tokio::time::Duration> {
    let s = s.trim();
    if let Some(h) = s.strip_suffix('h') {
        Ok(tokio::time::Duration::from_secs(h.parse::<u64>()? * 3600))
    } else if let Some(m) = s.strip_suffix('m') {
        Ok(tokio::time::Duration::from_secs(m.parse::<u64>()? * 60))
    } else if let Some(s_val) = s.strip_suffix('s') {
        Ok(tokio::time::Duration::from_secs(s_val.parse::<u64>()?))
    } else {
        anyhow::bail!("Invalid duration format: {} (use e.g. 6h, 30m, 60s)", s)
    }
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
