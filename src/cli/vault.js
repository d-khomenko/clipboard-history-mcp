import { join } from 'node:path';
import { homedir } from 'node:os';
import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';

const DB = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp', 'history.db');

export function vault(sub, id) {
  const store = new Store(DB, { masterKey: getOrCreateMasterKey() });
  try {
    if (sub === 'list') {
      const items = store.list({ limit: 500 }).filter((i) => i.primaryKind.startsWith('secret:'));
      console.log(JSON.stringify({ count: items.length, items }, null, 2));
    } else if (sub === 'unlock') {
      if (!id) { console.error('Usage: vault unlock <id>'); process.exit(1); }
      console.log(store.unlockSecret(parseInt(id, 10)));
    } else {
      console.error(`Unknown vault subcommand: ${sub}`);
      process.exit(1);
    }
  } finally {
    store.close();
  }
}
