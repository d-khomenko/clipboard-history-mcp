import { defineTool } from './index.js';
defineTool({
  name: 'get_json',
  description: 'Return JSON clips with parsed structure preview.',
  inputSchema: {
    type: 'object',
    properties: { limit: { type: 'number', minimum: 1, maximum: 100 } },
  },
  handler: ({ limit = 20 }, { store }) => {
    const items = store.list({ kind: 'json', limit });
    return {
      count: items.length,
      items: items.map((i) => {
        let parsed = null;
        try { parsed = JSON.parse(i.text); } catch { /* keep null */ }
        return { ...i, parsed };
      }),
    };
  },
});
