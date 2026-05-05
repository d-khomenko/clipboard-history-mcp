import { spawnSync } from 'node:child_process';

export function captureContext({ withWindowTitle = false } = {}) {
  const frontApp = runOsa(
    `tell application "System Events" to get name of first process whose frontmost is true`
  );
  let windowTitle = null;
  if (withWindowTitle) {
    windowTitle = runOsa(
      `tell application "System Events" to tell (first process whose frontmost is true) to get name of front window`
    );
  }
  return { frontApp, windowTitle };
}

function runOsa(script) {
  const r = spawnSync('osascript', ['-e', script], { encoding: 'utf8', timeout: 1000 });
  if (r.status !== 0) return null;
  const out = r.stdout?.trim();
  return out && out.length > 0 ? out : null;
}
