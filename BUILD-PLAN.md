# RescueClaw Build Plan

> Generated 2026-02-22. Each item lists exact files, implementation details, dependencies, and complexity.

---

## Phase 1: Make It Run End-to-End

### 1.1 Setup Wizard — Full Interactive Flow

**Files to modify:** `src/config/mod.rs`
**New files:** `rescueclaw.example.json`
**Complexity:** Medium (200-300 lines)
**Dependencies:** None

Replace the `setup_wizard()` stub with a full interactive terminal flow using stdin/stdout (no TUI crate needed — just `print!` + `std::io::stdin().read_line()`).

**Step-by-step flow:**

```
Step 1/6: Detect OpenClaw
```
- Call existing `detect_openclaw_workspace()` — already works
- Also detect config path: check `~/.openclaw/` then `~/.clawdbot/`
- Read OpenClaw's config to confirm it's valid (call `Config::read_openclaw_providers()` logic)
- Print: workspace path, config path, whether gateway is running (hit `http://127.0.0.1:7744/api/status`)
- Allow user to override with manual input if auto-detect fails

```
Step 2/6: Telegram Bot
```
- Print instructions: "Open @BotFather on Telegram, send /newbot, name it RescueClaw"
- Prompt: "Paste your bot token: "
- Validate token format: `digits:alphanumeric` (basic regex check)
- Test token by calling `https://api.telegram.org/bot<token>/getMe` via reqwest
- If invalid, print error and re-prompt
- Prompt: "Send /start to your bot, then tell me your Telegram user ID (or send /id to @userinfobot): "
- Store as `allowed_users: [user_id]`

```
Step 3/6: Backup Settings
```
- Prompt with defaults shown in brackets:
  - "Backup interval [6h]: "
  - "Max snapshots to keep [10]: "
  - "Backup path [/var/rescueclaw/backups]: "
  - "Include session files? [n]: "
- Validate interval format (must match `\d+[hms]`)
- Check backup path is writable (create dir, write test file, delete)

```
Step 4/6: Health Check Settings
```
- Prompt with defaults:
  - "Health check interval [5m]: "
  - "Failures before auto-restore [3]: "
  - "Enable auto-restore? [y]: "
- Validate interval format

```
Step 5/6: Write Config
```
- Build `Config` struct from collected values
- Serialize to JSON with `serde_json::to_string_pretty`
- Write to `~/.config/rescueclaw/rescueclaw.json` (create dirs)
- Print: "✓ Config written to ~/.config/rescueclaw/rescueclaw.json"

```
Step 6/6: First Backup & Service Install
```
- Take first backup via `backup::take_snapshot()`
- Print backup result
- Ask: "Install systemd service? [y]: "
- If yes, call `install_systemd_service()` (see 1.2)
- Print summary of everything configured

**Implementation details:**
- Add helper fn `prompt(question: &str, default: &str) -> String` that prints the question, reads a line, returns default if empty
- Add helper fn `prompt_yn(question: &str, default: bool) -> bool`
- The wizard must be `async` because it calls reqwest to validate the Telegram token
- On error at any step, print the error and let the user retry (loop)

**Create `rescueclaw.example.json`:**
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
    "autoRestore": true,
    "autoRestoreCooldown": "1h"
  },
  "telegram": {
    "token": "YOUR_BOT_TOKEN",
    "allowedUsers": [123456789]
  },
  "openclaw": {
    "workspace": "/home/opc/clawd",
    "configPath": "/home/opc/.openclaw"
  }
}
```

---

### 1.2 Systemd Service — Template Generation + Install/Uninstall

**Files to modify:** `src/config/mod.rs` (add functions at bottom)
**Complexity:** Low (80-100 lines)
**Dependencies:** None

**Add these functions to `src/config/mod.rs`:**

**`fn generate_service_file(cfg: &Config) -> String`**
Returns the systemd unit file content:
```ini
[Unit]
Description=RescueClaw - AI Agent Watchdog
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/rescueclaw start
Restart=on-failure
RestartSec=10
Environment=RUST_LOG=info

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/rescueclaw {cfg.openclaw.workspace} {cfg.openclaw.config_path}
ProtectHome=read-only

[Install]
WantedBy=multi-user.target
```

- `ReadWritePaths` must include the backup path, workspace, and config path from the loaded config
- `ExecStart` path should use `which rescueclaw` result if available, else `/usr/local/bin/rescueclaw`

**`fn install_systemd_service(cfg: &Config) -> Result<()>`**
1. Generate service file content
2. Write to `/etc/systemd/system/rescueclaw.service` (requires sudo — use `Command::new("sudo").args(["tee", "/etc/systemd/system/rescueclaw.service"])` and pipe content via stdin)
3. Run `sudo systemctl daemon-reload`
4. Run `sudo systemctl enable rescueclaw`
5. Run `sudo systemctl start rescueclaw`
6. Print status

**Update existing `uninstall()` function:**
1. Run `sudo systemctl stop rescueclaw`
2. Run `sudo systemctl disable rescueclaw`
3. Run `sudo rm /etc/systemd/system/rescueclaw.service`
4. Run `sudo systemctl daemon-reload`
5. Print: "Backups preserved at {path}"

---

### 1.3 Install Script (`install.sh`)

**New file:** `install.sh` (project root)
**Complexity:** Low (60-80 lines)
**Dependencies:** 1.2 (references systemd), needs GitHub releases set up (3.2)

```bash
#!/usr/bin/env bash
set -euo pipefail

VERSION="${RESCUECLAW_VERSION:-latest}"
REPO="harman314/rescueclaw"
INSTALL_DIR="/usr/local/bin"

# Detect arch
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  ARCH="amd64" ;;
    aarch64) ARCH="arm64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
if [ "$OS" != "linux" ]; then
    echo "RescueClaw only supports Linux. Got: $OS"; exit 1
fi

# Download
if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/$REPO/releases/latest/download/rescueclaw-linux-$ARCH"
else
    URL="https://github.com/$REPO/releases/download/$VERSION/rescueclaw-linux-$ARCH"
fi

echo "🛟 Installing RescueClaw..."
echo "   Downloading from $URL"
curl -fsSL "$URL" -o /tmp/rescueclaw
chmod +x /tmp/rescueclaw
sudo mv /tmp/rescueclaw "$INSTALL_DIR/rescueclaw"

echo "   ✓ Installed to $INSTALL_DIR/rescueclaw"
echo ""

# Run setup wizard
rescueclaw setup
```

**Add to README.md:**
```
curl -fsSL https://raw.githubusercontent.com/harman314/rescueclaw/main/install.sh | bash
```

---

## Phase 2: The Smart Stuff

### 2.1 OpenClaw Skill (JavaScript)

**New directory:** `skill/`
**New files:**
- `skill/SKILL.md`
- `skill/rescueclaw-checkpoint.js`
- `skill/package.json`

**Complexity:** Medium (150-200 lines total)
**Dependencies:** 1.1 (needs config written so daemon is running)

**Communication mechanism: File-based (simplest, no HTTP server needed in daemon)**

The skill writes checkpoint requests to a watched file; the daemon reads it.

**`skill/SKILL.md`:**
```markdown
# RescueClaw Skill

Provides checkpoint/rollback safety net for risky operations.

## Before Risky Operations
Before modifying config, installing skills, or updating the gateway, create a checkpoint:

\`\`\`javascript
const fs = require('fs');
const path = '/var/rescueclaw/checkpoint-request.json';
fs.writeFileSync(path, JSON.stringify({
  action: 'checkpoint',
  reason: 'About to apply config changes',
  timestamp: new Date().toISOString(),
  rollback_window_seconds: 300
}));
\`\`\`

RescueClaw will take an immediate backup. If the agent goes unresponsive within the rollback window, it auto-restores.

## After Successful Operations
Clear the checkpoint:
\`\`\`javascript
const fs = require('fs');
try { fs.unlinkSync('/var/rescueclaw/checkpoint-request.json'); } catch(e) {}
\`\`\`

## Commands
The agent can also invoke RescueClaw CLI directly:
- `rescueclaw backup` — manual snapshot
- `rescueclaw status` — check health
- `rescueclaw list` — list backups
```

**`skill/rescueclaw-checkpoint.js`:**
- Export functions: `createCheckpoint(reason)`, `clearCheckpoint()`, `getStatus()`
- Each writes/reads from `/var/rescueclaw/checkpoint-request.json`
- `getStatus()` runs `rescueclaw status` via `child_process.execSync` and returns parsed output

**`skill/package.json`:**
```json
{
  "name": "rescueclaw-skill",
  "version": "0.1.0",
  "description": "RescueClaw checkpoint API for OpenClaw agents",
  "main": "rescueclaw-checkpoint.js"
}
```

**Daemon-side changes (Rust):**

**Modify `src/health/mod.rs`** — in `health_loop()`, add checkpoint file watching:
- Every health check iteration, also check if `/var/rescueclaw/checkpoint-request.json` exists
- If found, read it, parse `action` field:
  - `checkpoint`: immediately call `backup::take_snapshot()`, set a rollback timer (store `rollback_deadline` in loop state)
  - During the rollback window, if agent goes unresponsive, auto-restore immediately (don't wait for `unhealthy_threshold`)
- If checkpoint file is gone (cleared by skill), cancel the rollback window

**Add a `CheckpointRequest` struct:**
```rust
#[derive(Debug, Deserialize)]
struct CheckpointRequest {
    action: String,
    reason: String,
    timestamp: String,
    rollback_window_seconds: u64,
}
```

---

### 2.2 Incident Analysis

**New file:** `src/analysis/mod.rs`
**Modify:** `src/main.rs` (add `mod analysis;`), `src/restore/mod.rs` (call analysis after restore), `src/telegram/mod.rs` (send report)
**Complexity:** Medium-High (200-250 lines)
**Dependencies:** 1.1 (needs config with OpenClaw provider info)

**Implementation:**

**`src/analysis/mod.rs`:**

**`pub async fn analyze_incident(cfg: &Config) -> Result<String>`**
1. Read OpenClaw provider config via `cfg.read_openclaw_providers()`
2. Extract API key and model from the provider config:
   - Parse `providers` JSON to find the configured provider (OpenRouter, Anthropic, etc.)
   - Get the `apiKey` and base URL
   - Use `default_model` or fall back to a cheap model
3. Gather evidence:
   - Read last 100 lines of OpenClaw gateway log: `~/.openclaw/gateway.log` (or wherever it is)
   - Read last 5 incident logs from `incidents.jsonl`
   - Diff current config vs backed-up config: compare `~/.openclaw/openclaw.json` with the one in the latest backup tarball
   - Read last modified files in workspace (check `memory/` dir for recent daily logs)
4. Build a prompt:
   ```
   You are an incident analyst for an AI agent system (OpenClaw).
   The agent became unresponsive and was auto-restored from backup.
   
   Analyze the following evidence and provide:
   1. Most likely root cause
   2. What changed before the failure
   3. Recommendations to prevent recurrence
   
   Evidence:
   [gateway log tail]
   [config diff]
   [recent incidents]
   [recent workspace changes]
   ```
5. Call the LLM API via reqwest:
   - POST to `{base_url}/chat/completions` with the prompt
   - Parse the response, extract the assistant message
6. Return the analysis as a formatted string

**`pub fn format_incident_report(analysis: &str, incident: &IncidentLog, backup_id: &str) -> String`**
- Format a human-readable report with sections:
  - 🚨 Incident Summary (timestamp, cause)
  - 🔍 Analysis (LLM output)
  - ✅ Recovery (which backup was restored)
  - 📋 Recommendations

**Integration in `src/restore/mod.rs`:**
After successful restore, call:
```rust
if let Ok(report) = analysis::analyze_incident(cfg).await {
    // Store report
    let report_path = cfg.backup.path.join(format!("incident-report-{}.md", backup_id));
    fs::write(&report_path, &report)?;
    // Return report for Telegram sending
}
```

**Integration in `src/health/mod.rs`:**
When auto-restore triggers in `health_loop()`:
1. After restore succeeds, call `analyze_incident()`
2. Send report via Telegram using a new helper (see below)

**Add to `src/telegram/mod.rs`:**
```rust
pub async fn send_notification(token: &str, chat_ids: &[i64], message: &str) -> Result<()> {
    let bot = Bot::new(token);
    for &chat_id in chat_ids {
        bot.send_message(ChatId(chat_id), message).await?;
    }
    Ok(())
}
```

This requires refactoring: the Telegram token and allowed_users need to be accessible from the health loop. Pass a `telegram::Notifier` struct (wrapping token + chat_ids) into the health loop.

**Refactoring needed in `src/main.rs`:**
```rust
async fn run_daemon(cfg: config::Config) -> Result<()> {
    let notifier = telegram::Notifier::new(&cfg.telegram);
    tokio::select! {
        r = health::health_loop(&cfg, &notifier) => r?,
        r = backup::backup_loop(&cfg) => r?,
        r = telegram::listen(&cfg) => r?,
    }
    Ok(())
}
```

---

### 2.3 Config Validation

**New file:** `src/validate/mod.rs`
**Modify:** `src/main.rs` (add `mod validate;`), `src/restore/mod.rs` (validate before applying)
**Complexity:** Low-Medium (100-150 lines)
**Dependencies:** None

**`src/validate/mod.rs`:**

**`pub fn validate_openclaw_config(config_path: &Path) -> Result<Vec<ValidationIssue>>`**
- Read `openclaw.json` (or `clawdbot.json`)
- Parse as `serde_json::Value`
- Check for:
  - `defaultModel` exists and is non-empty
  - `providers` exists and has at least one entry
  - Each provider has an `apiKey` that's non-empty and doesn't look like a placeholder (`YOUR_KEY_HERE`, `sk-xxx`)
  - `gateway` section exists
  - Port number is valid (1-65535)
  - No duplicate provider names
- Return list of `ValidationIssue { severity: Warning|Error, message: String }`

**`pub fn validate_workspace(workspace_path: &Path) -> Result<Vec<ValidationIssue>>`**
- Check `SOUL.md` exists (critical for agent identity)
- Check `AGENTS.md` exists
- Check `memory/` dir exists
- Warn if workspace is empty or very small

**`pub struct ValidationIssue`:**
```rust
pub enum Severity { Error, Warning }
pub struct ValidationIssue {
    pub severity: Severity,
    pub message: String,
}
```

**Integration in `src/restore/mod.rs`:**
Before `extract_backup()`, add validation step:
```rust
// Extract to temp dir first
let temp_dir = tempdir()?;
extract_backup_to(&snapshot.path, temp_dir.path())?;

// Validate
let issues = validate::validate_openclaw_config(&temp_dir.path().join("config"))?;
let errors: Vec<_> = issues.iter().filter(|i| matches!(i.severity, Severity::Error)).collect();
if !errors.is_empty() {
    println!("⚠ Config validation found errors:");
    for e in &errors { println!("  ❌ {}", e.message); }
    if !force { anyhow::bail!("Restore aborted. Use --force to override."); }
}
```

This requires adding a `--force` flag to the Restore CLI command and the `restore()` function signature.

**Modify `src/main.rs`:**
```rust
Commands::Restore {
    id: Option<String>,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    dry_run: bool,
},
```

**Dry-run mode:** Extract to temp dir, validate, print what would change, don't actually apply.

---

## Phase 3: Polish & Ship

### 3.1 Tests

**New files:**
- `tests/backup_restore.rs` (integration test)
- `src/config/mod.rs` (add `#[cfg(test)] mod tests`)
- `src/validate/mod.rs` (add `#[cfg(test)] mod tests`)
- `src/health/mod.rs` (add `#[cfg(test)] mod tests`)

**Complexity:** Medium (200-250 lines total)
**Dependencies:** 2.3 (validate module)

**`tests/backup_restore.rs` — Integration test:**
```rust
use tempfile::tempdir;

#[tokio::test]
async fn backup_restore_roundtrip() {
    // 1. Create a temp "workspace" with SOUL.md, AGENTS.md, memory/test.md
    // 2. Create a temp "config" dir with a fake openclaw.json
    // 3. Build a Config pointing to these temp dirs
    // 4. Call backup::take_snapshot() → verify tarball exists
    // 5. Modify a workspace file (change SOUL.md content)
    // 6. Extract the backup via restore::extract_backup()
    // 7. Verify SOUL.md is restored to original content
    // 8. Verify manifest.json was in the tarball
}

#[test]
fn backup_list_empty_dir() {
    // Config pointing to empty temp dir
    // list_snapshots() should return empty vec, not error
}

#[test]
fn backup_pruning() {
    // Create 12 fake backup files in temp dir
    // Config with max_snapshots=10
    // take_snapshot() should prune oldest 2
}
```

**Config unit tests (in `src/config/mod.rs`):**
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn default_config_is_valid() { ... }
    
    #[test]
    fn load_from_json_string() { 
        // Deserialize a known-good JSON string
    }
    
    #[test]
    fn detect_workspace_finds_soul_md() {
        // Create temp dir with SOUL.md, verify detection
    }
}
```

**Validate unit tests (in `src/validate/mod.rs`):**
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn valid_config_no_issues() { ... }
    
    #[test]
    fn missing_api_key_is_error() { ... }
    
    #[test]
    fn placeholder_api_key_is_warning() { ... }
    
    #[test]
    fn missing_default_model_is_error() { ... }
}
```

**Health check tests (in `src/health/mod.rs`):**
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn parse_health_interval_valid() {
        assert_eq!(parse_health_interval("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_health_interval("30s").unwrap(), Duration::from_secs(30));
    }
    
    #[test]
    fn parse_health_interval_invalid() {
        assert!(parse_health_interval("bad").is_err());
    }
    
    #[test]
    fn incident_log_roundtrip() {
        // Serialize IncidentLog to JSON, deserialize back, verify
    }
}
```

**Add to `Cargo.toml`:**
```toml
[dev-dependencies]
tempfile = "3"
```

---

### 3.2 CI/CD — GitHub Actions

**New file:** `.github/workflows/release.yml`
**Complexity:** Low (50-70 lines)
**Dependencies:** None

```yaml
name: Release

on:
  push:
    tags: ['v*']

jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            artifact: rescueclaw-linux-amd64
          - target: aarch64-unknown-linux-gnu
            artifact: rescueclaw-linux-arm64

    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install cross-compilation tools
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-aarch64-linux-gnu
          echo '[target.aarch64-unknown-linux-gnu]' >> ~/.cargo/config.toml
          echo 'linker = "aarch64-linux-gnu-gcc"' >> ~/.cargo/config.toml

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Rename binary
        run: cp target/${{ matrix.target }}/release/rescueclaw ${{ matrix.artifact }}

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact }}
          path: ${{ matrix.artifact }}

  release:
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@v4

      - name: Create Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            rescueclaw-linux-amd64/rescueclaw-linux-amd64
            rescueclaw-linux-arm64/rescueclaw-linux-arm64
          generate_release_notes: true
```

**Also add `.github/workflows/ci.yml`:**
```yaml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test
      - run: cargo clippy -- -D warnings
```

---

### 3.3 ClawHub Publishing

**New/modified files:**
- `skill/package.json` (already from 2.1, add clawhub metadata)
- `skill/SKILL.md` (already from 2.1)
- `skill/install.js` (post-install hook)

**Complexity:** Low (30-50 lines)
**Dependencies:** 2.1 (skill must exist)

**Update `skill/package.json`:**
```json
{
  "name": "rescueclaw-skill",
  "version": "0.1.0",
  "description": "RescueClaw - AI Agent Safety Net. Checkpoint API for safe config changes.",
  "main": "rescueclaw-checkpoint.js",
  "keywords": ["rescueclaw", "backup", "watchdog", "safety"],
  "repository": "https://github.com/harman314/rescueclaw",
  "clawhub": {
    "category": "safety",
    "postInstall": "install.js"
  }
}
```

**`skill/install.js`:**
```javascript
#!/usr/bin/env node
const { execSync } = require('child_process');
const fs = require('fs');

// Check if rescueclaw binary is installed
try {
    execSync('rescueclaw --version', { stdio: 'pipe' });
    console.log('✅ RescueClaw daemon is installed');
} catch {
    console.log('⚠️  RescueClaw daemon not found.');
    console.log('   Install it: curl -fsSL https://raw.githubusercontent.com/harman314/rescueclaw/main/install.sh | bash');
}

// Ensure checkpoint directory exists
const dir = '/var/rescueclaw';
if (!fs.existsSync(dir)) {
    console.log(`Creating ${dir}...`);
    try {
        fs.mkdirSync(dir, { recursive: true });
    } catch {
        console.log(`⚠️  Could not create ${dir}. Run: sudo mkdir -p ${dir} && sudo chown $(whoami) ${dir}`);
    }
}
```

---

## Execution Order

```
Phase 1 (sequential):
  1.1 Setup Wizard ──→ 1.2 Systemd Service ──→ 1.3 Install Script

Phase 2 (parallel after Phase 1):
  2.1 OpenClaw Skill    ┐
  2.2 Incident Analysis  ├── can be done in parallel
  2.3 Config Validation  ┘

Phase 3 (after Phase 2):
  3.1 Tests (after 2.3)
  3.2 CI/CD (independent, can start anytime)
  3.3 ClawHub (after 2.1)
```

**Total estimated new/modified lines:** ~1200-1500 lines across Rust + JS + YAML + Shell

## Critical Decisions

1. **Checkpoint communication:** File-based (`/var/rescueclaw/checkpoint-request.json`) — no HTTP server in daemon, keeps binary tiny
2. **LLM for analysis:** Reuse OpenClaw's configured provider — no new API keys needed
3. **Config format:** JSON (not TOML) — matches OpenClaw's own config format, already set up in existing code
4. **Systemd only:** No init.d, no launchd — Linux-only tool
5. **No TUI framework:** Plain stdin/stdout for setup wizard — zero extra dependencies
6. **Cross-compilation:** Use `gcc-aarch64-linux-gnu` in CI, not `cross` tool — simpler, fewer dependencies
