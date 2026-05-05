import { defineTool } from './index.js';
defineTool({
  name: 'search_history',
  description: 'Full-text search clipboard history (FTS5 BM25). Searches across preview, window title, and primary kind. Secret values still hidden.',
  inputSchema: {
    type: 'object',
    properties: {
      query: { type: 'string' },
      limit: { type: 'number', minimum: 1, maximum: 200 },
    },
    required: ['query'],
  },
  handler: ({ query, limit }, { store }) => {
    const items = store.search({ query, limit: limit ?? 20 });
    return { count: items.length, items };
  },
});
