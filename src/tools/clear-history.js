import { defineTool } from './index.js';
defineTool({
  name: 'clear_history',
  description: "Wipe history. scope: 'all' | 'older_than_days:N' | 'kind:K' (e.g. 'kind:secret:openai_api_key').",
  inputSchema: {
    type: 'object',
    properties: { scope: { type: 'string' } },
    required: ['scope'],
  },
  handler: ({ scope }, { store }) => {
    const removed = store.clear({ scope });
    return { ok: true, removed };
  },
});
