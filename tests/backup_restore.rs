use rescueclaw::*;
use tempfile::tempdir;
use std::fs;
use std::path::PathBuf;

fn create_test_config(temp_path: PathBuf) -> config::Config {
    config::Config {
        backup: config::BackupConfig {
            interval: "1h".to_string(),
            max_snapshots: 10,
            path: temp_path.join("backups"),
            include_sessions: false,
        },
        health: config::HealthConfig {
            check_interval: "5m".to_string(),
            unhealthy_threshold: 3,
            auto_restore: false,
            auto_restore_cooldown: Some("1h".to_string()),
        },
        telegram: config::TelegramConfig {
            token: "test_token".to_string(),
            allowed_users: vec![123456789],
        },
        openclaw: config::OpenClawConfig {
            workspace: temp_path.join("workspace"),
            config_path: temp_path.join("config"),
        },
    }
}

fn setup_test_workspace(workspace: &PathBuf) {
    fs::create_dir_all(workspace).unwrap();
    fs::write(workspace.join("SOUL.md"), "# Test Agent Soul\n").unwrap();
    fs::write(workspace.join("AGENTS.md"), "# Test Agents\n").unwrap();
    fs::create_dir_all(workspace.join("memory")).unwrap();
    fs::write(workspace.join("memory/test.md"), "# Test memory\n").unwrap();
}

fn setup_test_config_dir(config_dir: &PathBuf) {
    fs::create_dir_all(config_dir).unwrap();
    let test_config = serde_json::json!({
        "defaultModel": "gpt-4",
        "providers": {
            "openai": {
                "apiKey": "sk-test123",
                "baseUrl": "https://api.openai.com/v1"
            }
        },
        "gateway": {
            "port": 7744
        }
    });
    fs::write(
        config_dir.join("openclaw.json"),
        serde_json::to_string_pretty(&test_config).unwrap()
    ).unwrap();
}

#[test]
fn test_backup_list_empty_dir() {
    let temp = tempdir().unwrap();
    let cfg = create_test_config(temp.path().to_path_buf());
    
    // Create backup dir but leave it empty
    fs::create_dir_all(&cfg.backup.path).unwrap();
    
    let snapshots = backup::list_snapshots(&cfg).unwrap();
    assert_eq!(snapshots.len(), 0);
}

#[test]
fn test_backup_creation() {
    let temp = tempdir().unwrap();
    let mut cfg = create_test_config(temp.path().to_path_buf());
    
    setup_test_workspace(&cfg.openclaw.workspace);
    setup_test_config_dir(&cfg.openclaw.config_path);
    fs::create_dir_all(&cfg.backup.path).unwrap();
    
    // Take a backup
    let snapshot = backup::take_snapshot(&cfg).unwrap();
    
    // Verify it exists
    assert!(snapshot.path.exists());
    assert!(snapshot.filename.ends_with(".tar.gz"));
    assert!(snapshot.file_count > 0);
    
    // List should now show 1 backup
    let snapshots = backup::list_snapshots(&cfg).unwrap();
    assert_eq!(snapshots.len(), 1);
}

#[test]
fn test_backup_pruning() {
    let temp = tempdir().unwrap();
    let mut cfg = create_test_config(temp.path().to_path_buf());
    cfg.backup.max_snapshots = 3;
    
    setup_test_workspace(&cfg.openclaw.workspace);
    setup_test_config_dir(&cfg.openclaw.config_path);
    fs::create_dir_all(&cfg.backup.path).unwrap();
    
    // Create 5 backups
    for i in 0..5 {
        // Modify a file to make each backup different
        fs::write(
            cfg.openclaw.workspace.join("memory/test.md"),
            format!("# Test memory {}\n", i)
        ).unwrap();
        
        backup::take_snapshot(&cfg).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    
    // Should only keep max_snapshots (3)
    let snapshots = backup::list_snapshots(&cfg).unwrap();
    assert_eq!(snapshots.len(), 3);
}

#[tokio::test]
async fn test_backup_restore_roundtrip() {
    let temp = tempdir().unwrap();
    let cfg = create_test_config(temp.path().to_path_buf());
    
    setup_test_workspace(&cfg.openclaw.workspace);
    setup_test_config_dir(&cfg.openclaw.config_path);
    fs::create_dir_all(&cfg.backup.path).unwrap();
    
    // Original content
    let original_soul = fs::read_to_string(cfg.openclaw.workspace.join("SOUL.md")).unwrap();
    
    // Take backup
    let snapshot = backup::take_snapshot(&cfg).unwrap();
    
    // Modify workspace
    fs::write(
        cfg.openclaw.workspace.join("SOUL.md"),
        "# MODIFIED SOUL\n"
    ).unwrap();
    
    let modified_soul = fs::read_to_string(cfg.openclaw.workspace.join("SOUL.md")).unwrap();
    assert_ne!(original_soul, modified_soul);
    
    // NOTE: Full restore test requires OpenClaw gateway to be running
    // For unit test, we just verify extraction works
    let extract_temp = tempdir().unwrap();
    
    // Extract manually to verify tarball structure
    use flate2::read::GzDecoder;
    let tar_file = fs::File::open(&snapshot.path).unwrap();
    let decoder = GzDecoder::new(tar_file);
    let mut archive = tar::Archive::new(decoder);
    
    let mut found_soul = false;
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        let path = entry.path().unwrap();
        if path.to_string_lossy().contains("SOUL.md") {
            found_soul = true;
        }
    }
    
    assert!(found_soul, "SOUL.md should be in backup");
}
