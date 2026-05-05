import { spawnSync } from 'node:child_process';
import { writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '../..');
const LABEL = 'me.kz.clipboard-history';
const PLIST_PATH = join(homedir(), 'Library', 'LaunchAgents', `${LABEL}.plist`);
const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const LOG_PATH = join(DATA_DIR, 'daemon.log');

export function install({ windowTitles = false } = {}) {
  mkdirSync(DATA_DIR, { recursive: true });
  const tpl = readFileSync(join(ROOT, 'scripts/launchd.plist.template'), 'utf8');
  const env = { CLIPBOARD_CAPTURE_WINDOW_TITLE: windowTitles ? '1' : '0' };
  const envDict = Object.entries(env).map(
    ([k, v]) => `    <key>${k}</key>\n    <string>${v}</string>`
  ).join('\n');
  const plist = tpl
    .replace('__LABEL__', LABEL)
    .replace('__NODE__', process.execPath)
    .replace('__DAEMON_JS__', join(ROOT, 'src/daemon/index.js'))
    .replace('__ENV_DICT__', envDict)
    .replace(/__LOG_PATH__/g, LOG_PATH);
  writeFileSync(PLIST_PATH, plist);
  spawnSync('launchctl', ['unload', PLIST_PATH], { stdio: 'ignore' });
  const r = spawnSync('launchctl', ['load', '-w', PLIST_PATH], { stdio: 'inherit' });
  if (r.status !== 0) {
    console.error('launchctl load failed; check permissions.');
    process.exit(1);
  }
  console.log(`Installed → ${PLIST_PATH}`);
  console.log(`Logs    → ${LOG_PATH}`);
}

export function start() {
  spawnSync('launchctl', ['kickstart', '-k', `gui/${process.getuid()}/${LABEL}`], { stdio: 'inherit' });
}

export function stop() {
  spawnSync('launchctl', ['unload', PLIST_PATH], { stdio: 'inherit' });
}
