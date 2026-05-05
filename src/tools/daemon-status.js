import { defineTool } from './index.js';
defineTool({
  name: 'daemon_status',
  description: 'Is the clipboard-history-daemon running? When did it start? When was the last capture?',
  inputSchema: { type: 'object', properties: {} },
  handler: (_args, { daemonStatus }) => daemonStatus(),
});
