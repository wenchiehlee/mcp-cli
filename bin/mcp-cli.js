#!/usr/bin/env node
'use strict';

const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const binaryName = process.platform === 'win32' ? 'mcp-cli.exe' : 'mcp-cli';
const binaryPath = path.join(__dirname, '..', 'vendor', binaryName);

if (!fs.existsSync(binaryPath)) {
  console.error('mcp-cli native binary is not installed.');
  console.error('Run `npm rebuild @wenchiehlee1020/mcp-cli` to download it again.');
  process.exit(1);
}

// Auto-load .env if exists (current directory or ~/.config/mcp/.env)
function loadEnvFile(filePath) {
  if (fs.existsSync(filePath)) {
    try {
      const content = fs.readFileSync(filePath, 'utf8');
      for (const line of content.split(/\r?\n/)) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('#')) continue;
        const eq = trimmed.indexOf('=');
        if (eq > 0) {
          const k = trimmed.slice(0, eq).trim();
          let v = trimmed.slice(eq + 1).trim();
          if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) {
            v = v.slice(1, -1);
          }
          if (!process.env[k]) {
            process.env[k] = v;
          }
        }
      }
    } catch (_) {}
  }
}

loadEnvFile(path.join(process.cwd(), '.env'));
const homeDir = process.env.HOME || process.env.USERPROFILE;
if (homeDir) {
  loadEnvFile(path.join(homeDir, '.config', 'mcp', '.env'));
}

const child = spawn(binaryPath, process.argv.slice(2), { stdio: 'inherit', env: process.env });

child.on('error', (error) => {
  console.error(error.message);
  process.exit(1);
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
