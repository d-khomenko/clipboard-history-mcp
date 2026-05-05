import { defineTool } from './index.js';
defineTool({
  name: 'unlock_secret',
  description: 'Decrypt and return a stored secret. The reason argument is mandatory and surfaces in tool-call logs.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'number' },
      reason: { type: 'string', minLength: 3 },
    },
    required: ['id', 'reason'],
  },
  handler: ({ id, reason }, { store, log }) => {
    log?.(`unlock_secret id=${id} reason="${reason}"`);
    const value = store.unlockSecret(Number(id));
    const item = store.getItem(Number(id));
    return { value, kind: item.primaryKind, lastChars: value.slice(-Math.min(6, value.length)) };
  },
});
