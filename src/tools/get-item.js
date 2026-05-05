import { defineTool } from './index.js';
defineTool({
  name: 'get_item',
  description: 'Fetch one clipboard entry by id. For secrets returns metadata only — call unlock_secret to decrypt.',
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'number' } },
    required: ['id'],
  },
  handler: ({ id }, { store }) => {
    const item = store.getItem(Number(id));
    if (!item) return { error: `not found: ${id}` };
    if (item.primaryKind?.startsWith('secret:')) {
      return { ...item, requiresUnlock: true };
    }
    return item;
  },
});
