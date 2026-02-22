mod backup;
mod config;
mod health;
mod restore;
mod telegram;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber;

#[derive(Parser)]
#[command(name = "rescueclaw")]
#[command(about = "Your AI agent's always-on safety net 🛟")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard
    Setup,
    /// Start the watchdog daemon
    Start,
    /// Show status of agent and watchdog
    Status,
    /// Take a backup snapshot now
    Backup,
    /// List available backup snapshots
    List,
    /// Restore from a backup
    Restore {
        /// Backup ID to restore (latest if omitted)
        id: Option<String>,
    },
    /// Show recent incident logs
    Logs {
        /// Number of entries to show
        #[arg(short, default_value = "10")]
        n: usize,
    },
    /// Uninstall watchdog service
    Uninstall,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let cfg = config::Config::load()?;

    match cli.command {
        Commands::Setup => {
            config::setup_wizard().await?;
        }
        Commands::Start => {
            println!("🛟 RescueClaw starting...");
            let cfg = config::Config::load()?;
            run_daemon(cfg).await?;
        }
        Commands::Status => {
            let status = health::check_status(&cfg).await?;
            println!("{}", status);
        }
        Commands::Backup => {
            let snapshot = backup::take_snapshot(&cfg)?;
            println!("✓ Backup saved: {}", snapshot.filename);
        }
        Commands::List => {
            let snapshots = backup::list_snapshots(&cfg)?;
            for s in snapshots {
                println!("  {} — {} ({}) {}", 
                    s.id, s.timestamp, s.size_human, 
                    if s.verified { "✓" } else { "✗" }
                );
            }
        }
        Commands::Restore { id } => {
            restore::restore(&cfg, id.as_deref()).await?;
        }
        Commands::Logs { n } => {
            let logs = health::recent_incidents(&cfg, n)?;
            for log in logs {
                println!("  {} │ {} │ {}", log.timestamp, log.cause, log.recovery);
            }
        }
        Commands::Uninstall => {
            config::uninstall()?;
        }
    }

    Ok(())
}

/// Main daemon loop: health checks, scheduled backups, Telegram listener
async fn run_daemon(cfg: config::Config) -> Result<()> {
    println!("  Watchdog PID: {}", std::process::id());
    println!("  Health check: every {}", cfg.health.check_interval);
    println!("  Backup: every {}", cfg.backup.interval);
    println!("  Telegram: listening for commands");
    println!();

    // Run all three loops concurrently
    tokio::select! {
        r = health::health_loop(&cfg) => r?,
        r = backup::backup_loop(&cfg) => r?,
        r = telegram::listen(&cfg) => r?,
    }

    Ok(())
}
