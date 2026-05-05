import { spawnSync } from 'node:child_process';
import { unlinkSync, existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

const LABEL = 'me.kz.clipboard-history';
const PLIST_PATH = join(homedir(), 'Library', 'LaunchAgents', `${LABEL}.plist`);
const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');

export function uninstall({ keepData = false } = {}) {
  if (existsSync(PLIST_PATH)) {
    spawnSync('launchctl', ['unload', PLIST_PATH], { stdio: 'ignore' });
    unlinkSync(PLIST_PATH);
    console.log(`Removed ${PLIST_PATH}`);
  }
  if (!keepData && existsSync(DATA_DIR)) {
    rmSync(DATA_DIR, { recursive: true, force: true });
    console.log(`Removed ${DATA_DIR}`);
  }
}
