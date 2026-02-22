use anyhow::Result;
use teloxide::prelude::*;

use crate::config::Config;

/// Start the Telegram bot listener
pub async fn listen(cfg: &Config) -> Result<()> {
    let bot = Bot::new(&cfg.telegram.token);
    let allowed_users = cfg.telegram.allowed_users.clone();
    let cfg_clone = cfg.clone();

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let allowed = allowed_users.clone();
        let cfg = cfg_clone.clone();

        async move {
            let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);

            // Auth check
            if !allowed.is_empty() && !allowed.contains(&user_id) {
                bot.send_message(msg.chat.id, "⛔ Unauthorized").await?;
                return Ok(());
            }

            let text = msg.text().unwrap_or("");
            let response = handle_command(text, &cfg).await;
            bot.send_message(msg.chat.id, response).await?;

            Ok(())
        }
    })
    .await;

    Ok(())
}

/// Route Telegram commands to handlers
async fn handle_command(text: &str, cfg: &Config) -> String {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let cmd = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();

    match cmd.as_str() {
        "/start" | "/help" => help_text(),
        "/status" => cmd_status(cfg).await,
        "/rescue" => {
            let id = parts.get(1).copied();
            if id == Some("list") {
                cmd_list(cfg)
            } else {
                cmd_rescue(cfg, id).await
            }
        }
        "/backup" => cmd_backup(cfg),
        "/logs" => cmd_logs(cfg),
        "/rollback" => cmd_rescue(cfg, None).await, // rollback = restore latest
        "/health" => cmd_status(cfg).await,
        _ => "Unknown command. Try /help".to_string(),
    }
}

fn help_text() -> String {
    "🛟 *RescueClaw*\n\n\
     /status — Agent health & backup status\n\
     /rescue — Restore agent from latest backup\n\
     /rescue list — Show available backups\n\
     /rescue <id> — Restore specific backup\n\
     /backup — Take a snapshot now\n\
     /logs — Recent incidents\n\
     /rollback — Undo last change\n\
     /health — Detailed health report"
        .to_string()
}

async fn cmd_status(cfg: &Config) -> String {
    match crate::health::check_status(cfg).await {
        Ok(status) => format!("{}", status),
        Err(e) => format!("❌ Error checking status: {}", e),
    }
}

fn cmd_list(cfg: &Config) -> String {
    match crate::backup::list_snapshots(cfg) {
        Ok(snapshots) if snapshots.is_empty() => "No backups found.".to_string(),
        Ok(snapshots) => {
            let mut out = "📦 Available backups:\n\n".to_string();
            for (i, s) in snapshots.iter().enumerate().take(10) {
                out.push_str(&format!(
                    "{}. `{}` — {} ({})\n",
                    i + 1,
                    s.id,
                    s.timestamp,
                    s.size_human
                ));
            }
            out.push_str(
                "
Restore with: /rescue <id>",
            );
            out
        }
        Err(e) => format!("❌ Error listing backups: {}", e),
    }
}

fn cmd_backup(cfg: &Config) -> String {
    match crate::backup::take_snapshot(cfg) {
        Ok(snap) => format!(
            "✅ Backup saved!\n\nID: `{}`\nSize: {}\nFiles: {}",
            snap.id, snap.size_human, snap.file_count
        ),
        Err(e) => format!("❌ Backup failed: {}", e),
    }
}

async fn cmd_rescue(cfg: &Config, id: Option<&str>) -> String {
    let label = id.unwrap_or("latest");
    let _msg = format!(
        "🛟 Restoring from {} backup...\n\nThis may take 30 seconds.",
        label
    );

    match crate::restore::restore(cfg, id).await {
        Ok(_) => format!("✅ Agent restored and online!\n\nRestored from: {}", label),
        Err(e) => format!(
            "❌ Restore failed: {}\n\nYou may need to SSH in and fix manually.",
            e
        ),
    }
}

fn cmd_logs(cfg: &Config) -> String {
    match crate::health::recent_incidents(cfg, 5) {
        Ok(logs) if logs.is_empty() => "✅ No incidents recorded.".to_string(),
        Ok(logs) => {
            let mut out = "📋 Recent incidents:\n\n".to_string();
            for log in logs {
                out.push_str(&format!(
                    "• {} — {} ({})\n",
                    log.timestamp, log.cause, log.recovery
                ));
            }
            out
        }
        Err(e) => format!("❌ Error reading logs: {}", e),
    }
}
