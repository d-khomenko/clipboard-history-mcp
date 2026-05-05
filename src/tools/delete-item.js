import { defineTool } from './index.js';
defineTool({
  name: 'delete_item',
  description: 'Hard-delete a clip and (if secret) its encrypted blob.',
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'number' } },
    required: ['id'],
  },
  handler: ({ id }, { store }) => {
    store.delete(Number(id));
    return { ok: true };
  },
});
