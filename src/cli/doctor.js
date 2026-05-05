import { spawnSync } from 'node:child_process';
import { existsSync, accessSync, constants } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '../..');
const TYPES_BIN = join(ROOT, 'bin/pasteboard-types');
const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');

export function doctor() {
  const checks = [];

  checks.push(['data dir writable', () => {
    accessSync(DATA_DIR, constants.W_OK | constants.R_OK);
    return DATA_DIR;
  }]);

  checks.push(['Swift helper exists', () => {
    if (!existsSync(TYPES_BIN)) throw new Error(`build with: bash scripts/build-native.sh`);
    return TYPES_BIN;
  }]);

  checks.push(['osascript callable', () => {
    const r = spawnSync('osascript', ['-e', 'return 1'], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error(r.stderr);
    return 'ok';
  }]);

  checks.push(['Keychain accessible', () => {
    const r = spawnSync('security', ['list-keychains'], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error('security CLI failed');
    return 'ok';
  }]);

  checks.push(['accessibility permission for window titles', () => {
    const r = spawnSync('osascript', ['-e', 'tell application "System Events" to get name of front window of (first process whose frontmost is true)'], { encoding: 'utf8', timeout: 1500 });
    if (r.status !== 0 || /not authorized|1002/.test(r.stderr || '')) {
      throw new Error('accessibility permission missing — System Settings → Privacy → Accessibility');
    }
    return 'ok';
  }]);

  for (const [label, fn] of checks) {
    try {
      const out = fn();
      console.log(`✓ ${label}: ${out}`);
    } catch (e) {
      console.log(`✗ ${label}: ${e.message}`);
    }
  }
}
