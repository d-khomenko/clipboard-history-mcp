const REGISTRY = new Map();

export function defineTool({ name, description, inputSchema, handler }) {
  REGISTRY.set(name, { name, description, inputSchema, handler });
}

export function listTools() {
  return [...REGISTRY.values()].map(({ name, description, inputSchema }) => ({
    name, description, inputSchema,
  }));
}

export function callTool(name, args, ctx) {
  const tool = REGISTRY.get(name);
  if (!tool) throw new Error(`Unknown tool: ${name}`);
  return tool.handler(args || {}, ctx);
}

export async function loadAllTools() {
  await Promise.all([
    import('./list-history.js'),
    import('./get-item.js'),
    import('./search-history.js'),
    import('./get-urls.js'),
    import('./get-code.js'),
    import('./get-json.js'),
    import('./get-secrets-index.js'),
    import('./unlock-secret.js'),
    import('./copy-item.js'),
    import('./pin-item.js'),
    import('./tag-item.js'),
    import('./delete-item.js'),
    import('./clear-history.js'),
    import('./get-stats.js'),
    import('./daemon-status.js'),
  ]);
}
