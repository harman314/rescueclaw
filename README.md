# 🛟 RescueClaw

**Your AI agent's always-on safety net.**

> Your OpenClaw agent just broke itself installing a skill at 2 AM. You're in bed, phone in hand. You type `/rescue` in Telegram. 30 seconds later, it's back.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Powered by ZeroClaw](https://img.shields.io/badge/runtime-ZeroClaw%20🦀-orange)](https://github.com/openagen/zeroclaw)
[![ClawHub](https://img.shields.io/badge/ClawHub-rescue--bot-blue)](https://clawhub.com)

---

## The Problem

AI agents break themselves. A bad config change, a corrupted memory file, a skill install gone wrong — and suddenly your assistant is unreachable. Recovery means SSH-ing into a server, diagnosing the issue, and manually restoring files. At 2 AM. From your phone. Good luck.

**The worst part?** The agent that's supposed to help you... is the one that's down.

## The Solution

RescueClaw is a **separate, ultra-lightweight watchdog** that runs alongside your OpenClaw agent. Built on [ZeroClaw](https://github.com/openagen/zeroclaw) (~5MB RAM), it's so minimal it basically can't break.

It watches your agent. It takes backups. And when things go wrong, it brings your agent back — with a single command from your phone.

## How It Works

```
You (Telegram)          RescueClaw (ZeroClaw)         OpenClaw Agent
      │                       │                            │
      │  /rescue              │                            │ ✗ (dead)
      │──────────────────────►│                            │
      │                       │  restore from last backup  │
      │                       │───────────────────────────►│
      │                       │  restart gateway           │
      │                       │───────────────────────────►│
      │  "I'm back! Here's    │                            │ ✓ (alive)
      │   what happened..."   │◄───────────────────────────│
      │◄──────────────────────│                            │
```

## Installation

### Option A: One-liner (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/openagen/rescueclaw/main/install.sh | bash
```

This downloads the rescueclaw binary for your platform, runs the setup wizard, and gets you protected in under 2 minutes.

### Option B: Via ClawHub (from inside your OpenClaw agent)

Tell your agent:
> "Install rescueclaw from ClawHub"

Or manually:
```bash
clawhub install rescueclaw
```

The ClawHub install sets up both the OpenClaw skill (checkpoint API) and the ZeroClaw watchdog daemon.

### Option C: Manual install

```bash
# 1. Download the binary
# Linux amd64
curl -L https://github.com/openagen/rescueclaw/releases/latest/download/rescueclaw-linux-amd64 -o /usr/local/bin/rescueclaw
# Linux arm64 (Raspberry Pi, Oracle Cloud ARM, etc.)
curl -L https://github.com/openagen/rescueclaw/releases/latest/download/rescueclaw-linux-arm64 -o /usr/local/bin/rescueclaw

chmod +x /usr/local/bin/rescueclaw

# 2. Run setup wizard
rescueclaw setup
```

### Setup Wizard

The interactive setup takes about 60 seconds:

```
$ rescueclaw setup

🛟 RescueClaw Setup
━━━━━━━━━━━━━━━━━━

Step 1/5: Detect OpenClaw
  ✓ Found OpenClaw workspace at /home/opc/clawd
  ✓ Found OpenClaw config at ~/.openclaw
  ✓ Gateway is running (PID 4821)

Step 2/5: Telegram Bot
  To receive /rescue commands, you need a Telegram bot.
  
  1. Open Telegram and message @BotFather
  2. Send /newbot
  3. Name it something like "My RescueClaw"
  4. Paste the token here
  
  Token: 7481923xxx:AAH_xxxxxxxxxxxxxxxxxxxx
  ✓ Bot connected: @my_rescueclaw
  
  Now send any message to @my_rescueclaw in Telegram...
  ✓ Your Telegram user ID: 1618546873

Step 3/5: Backup Settings
  Backup location [/var/rescueclaw/backups]: 
  Backup interval [6h]: 
  Max snapshots to keep [10]: 
  Include chat sessions? (large) [n]: 
  ✓ Config saved

Step 4/5: Install Watchdog Service
  ✓ Created systemd service: rescueclaw.service
  ✓ Service started and enabled on boot

Step 5/5: First Backup
  ✓ Snapshot taken: backup-2026-02-22-1530-abc123.tar.gz (12MB)
  ✓ Verified: all 47 files intact

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🎉 RescueClaw is live!

  Watchdog:     running (PID 9182, 4.8MB RAM)
  First backup: /var/rescueclaw/backups/backup-2026-02-22-1530-abc123.tar.gz
  Next backup:  in 6 hours
  Health check: every 5 minutes
  
  Open Telegram → message @my_rescueclaw → try /status

  Tip: Install the OpenClaw skill for pre-action checkpoints:
    clawhub install rescueclaw-skill
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### Install the OpenClaw Skill (optional but recommended)

The watchdog works standalone, but adding the skill enables **pre-action checkpoints** — your agent automatically saves a snapshot before risky operations.

```bash
clawhub install rescueclaw-skill
```

Or tell your agent:
> "Install rescueclaw-skill from ClawHub and enable it"

Once installed, your agent will auto-checkpoint before:
- Config changes (`gateway config.apply`)
- Skill installs/updates (`clawhub install/update`)  
- Self-updates (`gateway update.run`)
- Any operation flagged as risky

### Verify Everything Works

```bash
# Check watchdog status
rescueclaw status

# Or from Telegram
/status
```

```
🛟 RescueClaw Status

Agent:       ✅ OpenClaw online (uptime: 3d 14h)
Watchdog:    ✅ Running (4.8MB RAM)  
Last backup: 2 minutes ago (12MB)
Backups:     1/10 slots used
Health:      3/3 checks passed
Skill:       ✅ Installed (checkpoint API active)
```

### Uninstall

```bash
rescueclaw uninstall          # removes watchdog service
clawhub uninstall rescueclaw-skill  # removes OpenClaw skill
```

Backups are preserved at `/var/rescueclaw/backups/` — delete manually if you want them gone.

## Commands

From your Telegram chat with the RescueClaw:

| Command | What it does |
|---------|-------------|
| `/status` | Is the agent alive? Last backup? Health score |
| `/rescue` | One-tap restore from latest healthy backup |
| `/rescue list` | Show available backup snapshots |
| `/rescue <id>` | Restore a specific backup |
| `/backup` | Take a snapshot right now |
| `/logs` | Recent incidents and errors |
| `/rollback` | Undo the last config/skill change |
| `/health` | Detailed health report |

## What Gets Backed Up

| Component | Included | Notes |
|-----------|----------|-------|
| Agent config | ✅ | `~/.openclaw/` |
| Memory files | ✅ | MEMORY.md, memory/*.md |
| Soul & identity | ✅ | SOUL.md, IDENTITY.md, AGENTS.md, USER.md |
| Skills | ✅ | Installed skill configs |
| Cron jobs | ✅ | Scheduled tasks |
| Custom scripts | ✅ | Workspace scripts/ |
| Sessions/chat history | ⚙️ | Optional (can be large) |
| Credentials/tokens | 🔒 | Encrypted separately |

Backups are compressed tarballs (~5-20MB each). Default: keep last 10 snapshots.

## Architecture

RescueClaw has two components:

### 1. The Watchdog (ZeroClaw daemon)
- Runs as a systemd service, completely independent of OpenClaw
- Own Telegram bot token — receives commands even when your agent is dead
- Health checks every 5 minutes (configurable)
- Scheduled backups every 6 hours (configurable)
- ~5MB RAM, near-zero CPU

### 2. The Skill (OpenClaw plugin)
- Installed inside your OpenClaw agent via ClawHub
- Provides checkpoint API — agent calls it before risky operations
- Reports incidents to the watchdog
- Enables pre-action backups: "save before you break"

```
┌──────────────────────────────┐
│        Your Server           │
│                              │
│  ┌────────────────────┐      │
│  │  OpenClaw Agent     │     │
│  │  (~200MB RAM)       │     │
│  │                     │     │
│  │  ┌──────────────┐  │     │
│  │  │ rescueclaw   │  │     │
│  │  │ skill        │──┼──┐  │
│  │  └──────────────┘  │  │  │
│  └────────────────────┘  │  │
│                           │  │
│  ┌────────────────────┐  │  │
│  │  ZeroClaw Watchdog  │◄─┘  │
│  │  (~5MB RAM)         │     │
│  │  Own Telegram bot   │     │
│  └────────────────────┘     │
│                              │
│  📦 /var/rescueclaw/backups  │
└──────────────────────────────┘
```

## Pre-Action Checkpoints

The real power is **prevention**. With the OpenClaw skill installed, your agent automatically takes a checkpoint before:

- `gateway config.apply` — config changes
- `clawhub install/update` — skill installations
- `gateway update.run` — self-updates
- Any operation the agent flags as risky

```
Agent: "I'm about to install a new skill..."
  └──► rescueclaw skill: checkpoint()
        └──► Watchdog: snapshot saved ✓
Agent: *installs skill*
Agent: *breaks*
Watchdog: "Agent unresponsive. Last checkpoint: 30 seconds ago."
You: /rescue
Watchdog: *restores* → Agent is back ✓
```

## Incident Learning

Every failure is logged with context:
- What changed before the crash
- Which files were modified
- Error logs from OpenClaw
- Time to recovery

Over time, RescueClaw builds a **failure knowledge base**:

```
$ /logs

📋 Recent Incidents:
┌─────────────┬──────────────────────────┬──────────┐
│ When        │ Cause                    │ Recovery │
├─────────────┼──────────────────────────┼──────────┤
│ 2h ago      │ Bad config.apply         │ Auto 30s │
│ 3 days ago  │ Skill install corrupted  │ Manual   │
│ 1 week ago  │ Memory file syntax error │ Auto 10s │
└─────────────┴──────────────────────────┴──────────┘

🔍 Pattern: config.apply failures are 60% of incidents.
   Suggestion: Enable config validation pre-hook.
```

## Configuration

`rescueclaw.json`:
```json
{
  "backup": {
    "interval": "6h",
    "maxSnapshots": 10,
    "path": "/var/rescueclaw/backups",
    "includeSessions": false
  },
  "health": {
    "checkInterval": "5m",
    "unhealthyThreshold": 3,
    "autoRestore": false
  },
  "telegram": {
    "token": "from setup wizard",
    "allowedUsers": [1618546873]
  },
  "openclaw": {
    "workspace": "/home/user/clawd",
    "configPath": "~/.openclaw"
  }
}
```

### Zero Config for AI Features

RescueClaw **reads your OpenClaw config** for all AI-related settings — model provider, API keys, default model. No duplication, no drift.

```
rescueclaw                     OpenClaw config (~/.openclaw/)
    │                                │
    ├── reads provider config ◄──────┤  providers, API keys
    ├── reads model settings  ◄──────┤  default model
    ├── reads channel config  ◄──────┤  Telegram token (for skill comms)
    └── own config only for:         │
        ├── backup schedule          │
        ├── health thresholds        │
        └── rescue bot Telegram token│
```

This means:
- **Swap your model in OpenClaw** → RescueClaw picks it up automatically
- **Rotate API keys** → no second place to update
- **Incident analysis** uses whatever model you already pay for
- **Setup wizard only asks** for backup prefs + its own Telegram bot token

The only thing RescueClaw owns is its own Telegram bot token (separate from your agent's) and operational settings like backup interval. Everything AI flows from your existing OpenClaw config.
```

## Auto-Heal Mode (Experimental)

For the brave: enable `autoRestore` and the watchdog will automatically restore from the last healthy backup if the agent is unresponsive for 3 consecutive health checks (15 minutes by default).

```json
{
  "health": {
    "autoRestore": true,
    "autoRestoreCooldown": "1h"
  }
}
```

## Enterprise: Fleet Mode 🏢

*Coming in v2*

Monitor and protect hundreds of OpenClaw agents from a single dashboard.

```
┌─────────────────────────────────────────┐
│          RescueClaw Fleet               │
│                                         │
│  Agent 1: ✅ Healthy    Last backup: 2h │
│  Agent 2: ⚠️ Degraded  RAM: 95%        │
│  Agent 3: ✅ Healthy    Last backup: 1h │
│  Agent 4: ❌ Down       Auto-restoring  │
│  Agent 5: ✅ Healthy    Last backup: 30m│
│  ...                                    │
│  Agent N: ✅ Healthy    Last backup: 4h │
│                                         │
│  Fleet Health: 98.2%  |  Incidents: 3/w │
└─────────────────────────────────────────┘
```

- Central backup policies across all agents
- Cross-agent pattern detection ("Agent 12 just did what Agent 7 did before crashing")
- Preemptive intervention based on learned failure patterns
- Webhook alerts to Slack, PagerDuty, etc.

## Resource Usage

| Component | RAM | CPU | Disk |
|-----------|-----|-----|------|
| Watchdog daemon | ~5MB | ~0% idle | 5MB binary |
| Per backup | — | 2s spike | 5-20MB |
| 10 backups stored | — | — | ~100-200MB |
| **Total overhead** | **~5MB** | **negligible** | **~200MB** |

## Requirements

- Linux (amd64 or arm64) — works on Raspberry Pi!
- OpenClaw agent (any version)
- Telegram account (for commands)
- ~200MB disk for backups

## FAQ

**Q: What if the rescue bot itself crashes?**
A: It's a 5MB Rust binary managed by systemd. If it dies, systemd restarts it in under a second. It has no state to corrupt — config is a single JSON file, backups are plain tarballs.

**Q: Can I use it without Telegram?**
A: v1 is Telegram-first. CLI commands work locally too (`rescueclaw status`, `rescueclaw restore`). Discord and other channels are planned for v2.

**Q: Does it work with multiple OpenClaw instances?**
A: One watchdog per agent in v1. Fleet mode (v2) will support many-to-one monitoring.

**Q: Can the OpenClaw agent access/delete backups?**
A: Backups are stored outside the agent's workspace with separate permissions. The agent can trigger checkpoints via the skill, but can't delete backups.

**Q: Is this just cron + tar?**
A: At its core, yes. But the value is in the integration — pre-action checkpoints, health monitoring, one-tap Telegram restore, incident learning, and the guarantee that the rescue system survives when the agent doesn't.

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Key areas:
- ZeroClaw plugin development (Rust)
- OpenClaw skill development (JS)
- Additional channel support (Discord, Slack, etc.)
- Fleet mode architecture
- Testing and documentation

## License

MIT — use it, fork it, protect your agents.

---

**Built with 🦀 by the OpenClaw community.**

*Because the best safety net is the one that's always there.*
