import { defineTool } from './index.js';
defineTool({
  name: 'get_code',
  description: 'Return code clips, optionally filtered by language (js, python, go, rust, ...).',
  inputSchema: {
    type: 'object',
    properties: {
      language: { type: 'string' },
      limit: { type: 'number', minimum: 1, maximum: 100 },
    },
  },
  handler: ({ language, limit = 20 }, { store }) => {
    const kindFilter = language ? `code:${language.toLowerCase()}` : null;
    const items = kindFilter
      ? store.list({ kind: kindFilter, limit })
      : store.list({ limit }).filter((i) => i.primaryKind.startsWith('code:'));
    return { count: items.length, items };
  },
});
