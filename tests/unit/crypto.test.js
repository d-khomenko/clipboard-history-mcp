import { describe, it, expect } from 'vitest';
import { randomBytes } from 'node:crypto';
import { encrypt, decrypt } from '../../src/core/crypto.js';

describe('crypto: AES-256-GCM round-trip', () => {
  const key = randomBytes(32);

  it('encrypts and decrypts a UTF-8 string', () => {
    const plaintext = 'sk-proj-abcXYZ123';
    const { ciphertext, nonce } = encrypt(plaintext, key);

    expect(ciphertext).toBeInstanceOf(Buffer);
    expect(nonce).toBeInstanceOf(Buffer);
    expect(nonce.length).toBe(12);
    expect(ciphertext.length).toBeGreaterThan(plaintext.length);

    const decrypted = decrypt(ciphertext, nonce, key);
    expect(decrypted).toBe(plaintext);
  });

  it('rejects tampered ciphertext', () => {
    const { ciphertext, nonce } = encrypt('hello', key);
    ciphertext[0] ^= 0xff;
    expect(() => decrypt(ciphertext, nonce, key)).toThrow();
  });

  it('rejects wrong key', () => {
    const { ciphertext, nonce } = encrypt('hello', key);
    const wrongKey = randomBytes(32);
    expect(() => decrypt(ciphertext, nonce, wrongKey)).toThrow();
  });
});
