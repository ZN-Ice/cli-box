#!/usr/bin/env node

/**
 * cli-box wrapper — finds the platform binary and delegates.
 * Allows `npx cli-box` or global `cli-box` command to work.
 */

import { createRequire } from 'module';
import { spawn } from 'child_process';
import path from 'path';
import os from 'os';

const platform = process.platform === 'darwin' ? 'darwin' :
                 process.platform === 'win32' ? 'win32' : 'linux';
const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
const platformPkgName = `cli-box-${platform}-${arch}`;

let binPath;
try {
  const require = createRequire(import.meta.url);
  const pkgJsonPath = require.resolve(`${platformPkgName}/package.json`);
  const pkgDir = path.dirname(pkgJsonPath);
  binPath = path.join(pkgDir, 'bin', 'cli-box');
} catch (e) {
  // Fallback: check ~/.cli-box/bin/
  binPath = path.join(os.homedir(), '.cli-box', 'bin', 'cli-box');
}

const args = process.argv.slice(2);
const child = spawn(binPath, args, { stdio: 'inherit' });
child.on('exit', (code) => process.exit(code ?? 1));
child.on('error', (err) => {
  console.error(`Failed to run cli-box: ${err.message}`);
  console.error('Install via: npm install -g cli-box-skill');
  console.error('Or: bash <(curl -fsSL https://raw.githubusercontent.com/ZN-Ice/cli-box/main/skill/install.sh)');
  process.exit(1);
});
