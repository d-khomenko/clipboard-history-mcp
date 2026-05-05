import { defineTool } from './index.js';
defineTool({
  name: 'pin_item',
  description: 'Pin or unpin a clip so it stays at the top of list_history.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'number' },
      pinned: { type: 'boolean' },
    },
    required: ['id', 'pinned'],
  },
  handler: ({ id, pinned }, { store }) => {
    if (pinned) store.pin(Number(id));
    else store.unpin(Number(id));
    return { ok: true };
  },
});
