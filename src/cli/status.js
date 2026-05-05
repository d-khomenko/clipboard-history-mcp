import { existsSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { Store } from '../core/store.js';

const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const PID_FILE = join(DATA_DIR, 'daemon.pid');
const DB = join(DATA_DIR, 'history.db');

export function status() {
  const out = { dataDir: DATA_DIR, dbPath: DB };
  out.dbExists = existsSync(DB);
  if (out.dbExists) out.dbSizeBytes = statSync(DB).size;
  if (existsSync(PID_FILE)) {
    const pid = parseInt(readFileSync(PID_FILE, 'utf8'), 10);
    try { process.kill(pid, 0); out.daemon = { running: true, pid }; }
    catch { out.daemon = { running: false, stalePid: pid }; }
  } else out.daemon = { running: false };
  if (out.dbExists) {
    const store = new Store(DB);
    out.stats = store.stats();
    store.close();
  }
  console.log(JSON.stringify(out, null, 2));
}
