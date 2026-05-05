import { createCipheriv, createDecipheriv, randomBytes } from 'node:crypto';
import { spawnSync } from 'node:child_process';

const ALGO = 'aes-256-gcm';
const KEYCHAIN_SERVICE = 'clipboard-history-mcp';
const KEYCHAIN_ACCOUNT = 'master-key-v1';

export function encrypt(plaintext, key) {
  const nonce = randomBytes(12);
  const cipher = createCipheriv(ALGO, key, nonce);
  const enc = Buffer.concat([cipher.update(plaintext, 'utf8'), cipher.final()]);
  const tag = cipher.getAuthTag();
  return { ciphertext: Buffer.concat([enc, tag]), nonce };
}

export function decrypt(ciphertext, nonce, key) {
  const tag = ciphertext.subarray(ciphertext.length - 16);
  const enc = ciphertext.subarray(0, ciphertext.length - 16);
  const decipher = createDecipheriv(ALGO, key, nonce);
  decipher.setAuthTag(tag);
  return Buffer.concat([decipher.update(enc), decipher.final()]).toString('utf8');
}

export function getOrCreateMasterKey() {
  const existing = readKeychain();
  if (existing) return existing;
  const fresh = randomBytes(32);
  writeKeychain(fresh);
  return fresh;
}

function readKeychain() {
  const r = spawnSync('security', [
    'find-generic-password',
    '-s', KEYCHAIN_SERVICE,
    '-a', KEYCHAIN_ACCOUNT,
    '-w',
  ], { encoding: 'utf8' });
  if (r.status !== 0) return null;
  return Buffer.from(r.stdout.trim(), 'base64');
}

// Security note: the base64-encoded key is passed as a CLI argument to `security`,
// which briefly exposes it in the process argument list (visible via `ps aux` to
// same-uid processes). On macOS, ps output is restricted to the same UID by default,
// so the practical risk is low. A fully hardened approach would use a Swift Keychain
// helper that writes the secret via the Keychain API directly, bypassing argv
// entirely — tracked as a future v0.3 improvement.
function writeKeychain(key) {
  const r = spawnSync('security', [
    'add-generic-password',
    '-s', KEYCHAIN_SERVICE,
    '-a', KEYCHAIN_ACCOUNT,
    '-w', key.toString('base64'),
    '-U',
  ]);
  if (r.status !== 0) {
    throw new Error('Failed to write master key to Keychain');
  }
}
