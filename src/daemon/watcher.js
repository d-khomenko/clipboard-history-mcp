import { readClipboard, isTransient } from '../core/pasteboard.js';
import { classify } from '../core/types.js';
import { detectSecret } from '../core/secrets.js';
import { captureContext } from './context.js';

export function startWatcher(store, opts = {}) {
  const intervalMs = opts.intervalMs ?? 1500;
  const captureWindowTitle = opts.captureWindowTitle ?? false;
  const ignoreApps = new Set(opts.ignoreApps ?? []);
  const onError = opts.onError ?? ((e) => console.error('[watcher]', e.message));
  const neverStoreSecrets = opts.neverStoreSecrets ?? false;
  const log = opts.log ?? (() => {});

  let lastSeen = null;
  let stopped = false;

  const tick = async () => {
    if (stopped) return;
    try {
      const text = await readClipboard();
      if (!text || text === lastSeen) return;
      lastSeen = text;

      if (isTransient()) {
        log('skip: transient pasteboard type');
        return;
      }

      const { frontApp, windowTitle } = captureContext({ withWindowTitle: captureWindowTitle });
      if (frontApp && ignoreApps.has(frontApp)) {
        log(`skip: ignored app ${frontApp}`);
        return;
      }

      const secret = detectSecret(text);
      if (secret) {
        if (neverStoreSecrets) {
          log(`secret detected (paranoid mode, no ciphertext): ${secret.kind}`);
        } else {
          store.addSecret({
            text,
            secretKind: secret.kind,
            sourceApp: frontApp,
            windowTitle,
          });
          log(`secret captured: ${secret.kind} from ${frontApp}`);
        }
        return;
      }

      const { primaryKind, kinds } = classify(text);
      store.addClip({
        text,
        primaryKind,
        kinds,
        sourceApp: frontApp,
        windowTitle,
      });
      log(`clip captured: ${primaryKind} from ${frontApp ?? '?'}`);
    } catch (err) {
      onError(err);
    }
  };

  tick();
  const handle = setInterval(tick, intervalMs);
  return () => {
    stopped = true;
    clearInterval(handle);
  };
}
