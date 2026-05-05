import { defineTool } from './index.js';
defineTool({
  name: 'get_stats',
  description: 'Counts by kind, oldest/newest timestamps, db size.',
  inputSchema: { type: 'object', properties: {} },
  handler: (_args, { store, dbPath, statSync }) => {
    const s = store.stats();
    const size = statSync(dbPath).size;
    return { ...s, dbPath, sizeBytes: size };
  },
});
