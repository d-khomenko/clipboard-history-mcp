import { defineTool } from './index.js';
defineTool({
  name: 'get_secrets_index',
  description: 'List secret clips with metadata only — no values. Use unlock_secret(id, reason) to retrieve a value.',
  inputSchema: {
    type: 'object',
    properties: { kind: { type: 'string', description: 'filter by secret kind, e.g. openai_api_key' } },
  },
  handler: ({ kind }, { store }) => {
    const filter = kind ? `secret:${kind}` : null;
    const items = filter
      ? store.list({ kind: filter, limit: 200 })
      : store.list({ limit: 500 }).filter((i) => i.primaryKind.startsWith('secret:'));
    return {
      count: items.length,
      items: items.map((i) => ({
        id: i.id,
        secretKind: i.primaryKind.replace(/^secret:/, ''),
        sourceApp: i.sourceApp,
        windowTitle: i.windowTitle,
        firstCopiedAt: i.firstCopiedAt,
        lastCopiedAt: i.lastCopiedAt,
      })),
    };
  },
});
