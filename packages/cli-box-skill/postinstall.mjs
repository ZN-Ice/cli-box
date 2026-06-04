#!/usr/bin/env node

/**
 * cli-box postinstall script
 *
 * Runs after `npm install -g cli-box-skill`.
 * 1. Finds the installed platform package (cli-box-darwin-arm64)
 * 2. Creates symlinks in ~/.cli-box/bin/
 * 3. Installs SKILL.md to .claude/skills/cli-box/ and .opencode/skills/cli-box/
 */

import { createRequire } from 'module';
import fs from 'fs';
import path from 'path';
import os from 'os';

const home = os.homedir();
const binDir = path.join(home, '.cli-box', 'bin');

function info(msg) { console.log(`  ➜  ${msg}`); }
function ok(msg) { console.log(`  ✓  ${msg}`); }
function warn(msg) { console.warn(`  ⚠  ${msg}`); }

// 1. Detect platform
const platform = process.platform === 'darwin' ? 'darwin' :
                 process.platform === 'win32' ? 'win32' : 'linux';
const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
const platformPkgName = `cli-box-${platform}-${arch}`;

info(`Platform: ${platform}-${arch}`);

// 2. Find platform package
let platformPkgDir;
try {
  const require = createRequire(import.meta.url);
  const pkgJsonPath = require.resolve(`${platformPkgName}/package.json`);
  platformPkgDir = path.dirname(pkgJsonPath);
  ok(`Found platform package: ${platformPkgName}`);
} catch (e) {
  warn(`Platform package ${platformPkgName} not found. Skipping binary setup.`);
  warn('You can install binaries manually via: bash <(curl -fsSL https://raw.githubusercontent.com/ZN-Ice/cli-box/main/skill/install.sh)');
  process.exit(0);
}

// 3. Create symlinks
try {
  fs.mkdirSync(binDir, { recursive: true });

  const bins = ['cli-box', 'cli-box-daemon'];
  for (const bin of bins) {
    const src = path.join(platformPkgDir, 'bin', bin);
    const dst = path.join(binDir, bin);

    if (fs.existsSync(src)) {
      fs.rmSync(dst, { force: true });
      fs.symlinkSync(src, dst);
      fs.chmodSync(src, 0o755);
      ok(`${bin} → ${src}`);
    } else {
      warn(`${bin} not found in platform package`);
    }
  }
} catch (e) {
  warn(`Failed to create symlinks: ${e.message}`);
}

// 4. Install SKILL.md
const skillSrc = path.join(path.dirname(new URL(import.meta.url).pathname), 'skill', 'SKILL.md');

const targets = [
  path.join(home, '.claude', 'skills', 'cli-box'),
  path.join(home, '.config', 'opencode', 'skills', 'cli-box'),
];

for (const target of targets) {
  try {
    if (fs.existsSync(path.dirname(target))) {
      fs.mkdirSync(target, { recursive: true });
      fs.copyFileSync(skillSrc, path.join(target, 'SKILL.md'));
      ok(`SKILL.md → ${target}/`);
    }
  } catch (e) {
    // Silent — target harness may not be installed
  }
}

// 5. Done
console.log('');
console.log('  cli-box installed successfully!');
console.log('');
console.log('  Add to PATH:');
console.log(`    export PATH="$HOME/.cli-box/bin:$PATH"`);
console.log('');
console.log('  Quick start:');
console.log('    cli-box start claude');
console.log('    cli-box start zsh');
console.log('    cli-box list');
console.log('');
