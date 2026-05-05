import { defineTool } from './index.js';
defineTool({
  name: 'list_history',
  description: 'List recent clipboard entries, newest first. Secret values are never returned — for those, only metadata.',
  inputSchema: {
    type: 'object',
    properties: {
      limit: { type: 'number', minimum: 1, maximum: 200 },
      offset: { type: 'number', minimum: 0 },
      kind: { type: 'string', description: 'filter by primary or secondary kind' },
      source_app: { type: 'string' },
      since: { type: 'number', description: 'epoch ms; only items copied after this' },
      pinned_only: { type: 'boolean' },
    },
  },
  handler: ({ limit, offset, kind, source_app, since, pinned_only }, { store }) => {
    const items = store.list({ limit, offset, kind, sourceApp: source_app, since, pinnedOnly: pinned_only });
    return { count: items.length, items };
  },
});
