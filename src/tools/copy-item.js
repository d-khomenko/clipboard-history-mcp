import { defineTool } from './index.js';
import { writeClipboard } from '../core/pasteboard.js';
defineTool({
  name: 'copy_item',
  description: 'Restore a stored clip to the system clipboard so the user can paste it. For secrets, fails — use unlock_secret first.',
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'number' } },
    required: ['id'],
  },
  handler: async ({ id }, { store }) => {
    const item = store.getItem(Number(id));
    if (!item) return { error: `not found: ${id}` };
    if (!item.text) return { error: 'cannot restore secret directly; use unlock_secret', requiresUnlock: true };
    await writeClipboard(item.text);
    store.bumpPaste(Number(id));
    return { ok: true, id, length: item.length };
  },
});
