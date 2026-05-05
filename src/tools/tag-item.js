import { defineTool } from './index.js';
defineTool({
  name: 'tag_item',
  description: 'Add or remove a user tag on a clip.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'number' },
      tag: { type: 'string', minLength: 1 },
      remove: { type: 'boolean' },
    },
    required: ['id', 'tag'],
  },
  handler: ({ id, tag, remove }, { store }) => {
    if (remove) store.untag(Number(id), tag);
    else store.tag(Number(id), tag);
    return { ok: true };
  },
});
