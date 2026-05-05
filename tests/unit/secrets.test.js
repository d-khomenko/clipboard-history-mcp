import { describe, it, expect } from 'vitest';
import { detectSecret } from '../../src/core/secrets.js';

describe('detectSecret', () => {
  it('detects OpenAI API keys', () => {
    // Short-form OpenAI key: sk-<20 alphanum>T3BlbkFJ<20 alphanum>
    const key = 'sk-' + 'a'.repeat(20) + 'T3BlbkFJ' + 'a'.repeat(20);
    const r = detectSecret('My key is ' + key);
    expect(r).toBeTruthy();
    expect(r.kind).toMatch(/openai/i);
    expect(r.value).toContain('sk-');
    expect(r.lastChars.length).toBeGreaterThanOrEqual(4);
  });

  it('detects AWS access keys', () => {
    // AKIAIOSFODNN7EXAMPLE is blocked by gitleaks allowlist (.+EXAMPLE$)
    // Use AKIAIOSFODNN7EXAMPLB to avoid the allowlist
    const r = detectSecret('AKIAIOSFODNN7EXAMPLB');
    expect(r?.kind).toMatch(/aws/i);
  });

  it('detects JWT', () => {
    const jwt = 'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abc-_def123';
    const r = detectSecret(jwt);
    expect(r?.kind).toBe('jwt');
  });

  it('detects valid Luhn credit cards', () => {
    const r = detectSecret('4242 4242 4242 4242');
    expect(r?.kind).toBe('credit_card');
  });

  it('rejects invalid Luhn (random 16 digits)', () => {
    const r = detectSecret('1234 5678 9012 3456');
    if (r) expect(r.kind).not.toBe('credit_card');
  });

  it('returns null for plain text', () => {
    expect(detectSecret('just hello world')).toBeNull();
  });
});
