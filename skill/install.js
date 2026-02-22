#!/usr/bin/env node
/**
 * Post-install hook for RescueClaw skill
 * Checks that the RescueClaw daemon is installed
 */

const { execSync } = require('child_process');
const fs = require('fs');

console.log('🛟 RescueClaw Skill - Post-Install Check\n');

// Check if rescueclaw binary is installed
try {
  const version = execSync('rescueclaw --version', {
    encoding: 'utf-8',
    stdio: ['pipe', 'pipe', 'pipe']
  }).trim();
  console.log(`✅ RescueClaw daemon installed: ${version}`);
} catch {
  console.log('⚠️  RescueClaw daemon not found');
  console.log('   Install: curl -fsSL https://raw.githubusercontent.com/harman314/rescueclaw/main/install.sh | bash');
  console.log('');
}

// Ensure checkpoint directory exists
const dir = '/var/rescueclaw';
if (!fs.existsSync(dir)) {
  console.log(`📁 Creating checkpoint directory: ${dir}`);
  try {
    fs.mkdirSync(dir, { recursive: true, mode: 0o755 });
    console.log('   ✅ Directory created');
  } catch (err) {
    console.log(`   ⚠️  Could not create ${dir}`);
    console.log(`   Run: sudo mkdir -p ${dir} && sudo chown $(whoami) ${dir}`);
  }
} else {
  console.log(`✅ Checkpoint directory ready: ${dir}`);
}

console.log('\n🎯 Skill installed! Use rescueclaw-checkpoint.js for safe operations.');
