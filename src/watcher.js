import { readClipboard } from "./clipboard.js";

const DEFAULT_INTERVAL_MS = Number(process.env.CLIPBOARD_POLL_MS) || 1500;

export function startWatcher(store, { intervalMs = DEFAULT_INTERVAL_MS, onError } = {}) {
  let lastSeen = null;
  let stopped = false;

  const tick = async () => {
    if (stopped) return;
    try {
      const text = await readClipboard();
      if (text && text !== lastSeen) {
        lastSeen = text;
        store.add(text);
      }
    } catch (err) {
      if (onError) onError(err);
    }
  };

  tick();
  const handle = setInterval(tick, intervalMs);

  return () => {
    stopped = true;
    clearInterval(handle);
  };
}
