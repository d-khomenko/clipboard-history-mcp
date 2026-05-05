import { defineTool } from './index.js';
defineTool({
  name: 'get_urls',
  description: 'Return URL clips, deduped by hostname, ranked by paste_count + recency.',
  inputSchema: {
    type: 'object',
    properties: {
      limit: { type: 'number', minimum: 1, maximum: 100 },
      since: { type: 'number' },
    },
  },
  handler: ({ limit = 20, since }, { store }) => {
    const items = store.list({ kind: 'url', limit: 200, since });
    const dedup = new Map();
    for (const it of items) {
      try {
        const host = new URL(it.text).hostname;
        const existing = dedup.get(host);
        if (!existing || it.lastCopiedAt > existing.lastCopiedAt) dedup.set(host, it);
      } catch { /* skip invalid url */ }
    }
    return { count: dedup.size, items: [...dedup.values()].slice(0, limit) };
  },
});
