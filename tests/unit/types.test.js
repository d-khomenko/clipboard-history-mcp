import { describe, it, expect } from 'vitest';
import { classify } from '../../src/core/types.js';

describe('classify', () => {
  it('detects URLs', () => {
    const r = classify('https://api.openai.com/v1/chat?x=1');
    expect(r.primaryKind).toBe('url');
    expect(r.kinds).toContain('url');
  });

  it('detects emails', () => {
    expect(classify('hello@example.com').primaryKind).toBe('email');
  });

  it('detects valid JSON', () => {
    expect(classify('{"a":1,"b":[2,3]}').primaryKind).toBe('json');
  });

  it('detects SQL', () => {
    expect(classify('SELECT * FROM users WHERE id = 1').primaryKind).toBe('sql');
  });

  it('detects shell commands', () => {
    expect(classify('git checkout -b feature/x').primaryKind).toBe('shell');
    expect(classify('$ npm install').primaryKind).toBe('shell');
  });

  it('detects code with language hint', () => {
    const r = classify('def foo():\n    return 42\n');
    expect(r.primaryKind).toMatch(/^code:/);
    expect(r.code?.language?.toLowerCase()).toBe('python');
  });

  it('falls back to plain text', () => {
    expect(classify('just some thoughts').primaryKind).toBe('text');
  });

  it('returns multiple kinds when applicable', () => {
    const r = classify('See https://example.com for details');
    expect(r.kinds).toContain('url');
  });
});
