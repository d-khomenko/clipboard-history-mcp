#!/usr/bin/env node
import { writeFileSync, unlinkSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';
import { startWatcher } from './watcher.js';

const DEFAULT_DIR = process.env.CLIPBOARD_DATA_DIR
  || join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const DEFAULT_DB = process.env.CLIPBOARD_DB_PATH || join(DEFAULT_DIR, 'history.db');
const PID_FILE = join(DEFAULT_DIR, 'daemon.pid');

mkdirSync(DEFAULT_DIR, { recursive: true });

const masterKey = getOrCreateMasterKey();
const store = new Store(DEFAULT_DB, { masterKey });

writeFileSync(PID_FILE, String(process.pid));

const stop = startWatcher(store, {
  intervalMs: Number(process.env.CLIPBOARD_POLL_MS) || 1500,
  captureWindowTitle: process.env.CLIPBOARD_CAPTURE_WINDOW_TITLE === '1',
  ignoreApps: (process.env.CLIPBOARD_IGNORE_APPS || '').split(',').map(s => s.trim()).filter(Boolean),
  neverStoreSecrets: process.env.CLIPBOARD_NEVER_STORE_SECRETS === '1',
  log: (m) => process.stdout.write(`[daemon] ${new Date().toISOString()} ${m}\n`),
  onError: (e) => process.stderr.write(`[daemon] ${new Date().toISOString()} ERROR ${e.message}\n`),
});

const shutdown = () => {
  stop();
  store.close();
  if (existsSync(PID_FILE)) unlinkSync(PID_FILE);
  process.exit(0);
};
process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);

process.stdout.write(`[daemon] started pid=${process.pid} db=${DEFAULT_DB}\n`);
