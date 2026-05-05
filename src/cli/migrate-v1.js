import { join } from 'node:path';
import { homedir } from 'node:os';
import { existsSync, readFileSync } from 'node:fs';
import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';
import { classify } from '../core/types.js';

const V1_PATH = join(homedir(), '.clipboard-history-mcp', 'history.json');
const V2_DB = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp', 'history.db');

export function migrateV1() {
  if (!existsSync(V1_PATH)) {
    console.log('No v1 history.json found — nothing to migrate.');
    return;
  }
  const data = JSON.parse(readFileSync(V1_PATH, 'utf8'));
  const items = Array.isArray(data.items) ? data.items : [];
  const store = new Store(V2_DB, { masterKey: getOrCreateMasterKey() });
  let migrated = 0;
  for (const item of items.reverse()) {
    if (!item?.text) continue;
    const { primaryKind, kinds } = classify(item.text);
    store.addClip({ text: item.text, primaryKind, kinds });
    migrated++;
  }
  store.close();
  console.log(`Migrated ${migrated} clips from v1`);
}
