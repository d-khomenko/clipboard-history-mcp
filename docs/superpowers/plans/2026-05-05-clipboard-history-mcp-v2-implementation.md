# clipboard-history-mcp v2 — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the v1 single-process clipboard MCP with a launchd-managed daemon + thin stdio MCP backed by SQLite/FTS5, classifying every clip by type, encrypting detected secrets, and exposing a 15-tool LLM-friendly surface.

**Architecture:** Two Node processes (daemon + MCP) share a SQLite database in WAL mode. A 30-line Swift helper enumerates `NSPasteboard` types so we can honor the `transient`/`concealed` markers password managers set. AES-256-GCM at rest, key in macOS Keychain. Vendored gitleaks rules for secret detection, flourite for code-language detection.

**Tech Stack:** Node 20+, ESM, `@modelcontextprotocol/sdk`, `better-sqlite3` (FTS5), `@iarna/toml`, `flourite`, `commander`, `vitest`, Swift 5 (one helper), launchd.

**Reference spec:** [`docs/superpowers/specs/2026-05-05-clipboard-history-mcp-v2-design.md`](../specs/2026-05-05-clipboard-history-mcp-v2-design.md)

---

## File map (locked in)

```
src/
├── core/
│   ├── db.js          ← SQLite open + migrations + FTS5 + WAL
│   ├── store.js       ← high-level CRUD on top of db.js
│   ├── crypto.js      ← AES-GCM + Keychain key fetch
│   ├── secrets.js     ← gitleaks rules loader + Luhn + JWT
│   ├── types.js       ← URL/JSON/SQL/shell/code classifier
│   └── pasteboard.js  ← pbpaste/pbcopy + types probe
├── daemon/
│   ├── index.js       ← daemon entry, signals, supervisor
│   ├── watcher.js     ← poll loop driving capture
│   └── context.js     ← frontApp + windowTitle (osascript)
├── mcp/
│   └── index.js       ← stdio server, tool dispatch
├── tools/
│   ├── index.js       ← registry exposing tools[] + handlers
│   ├── list-history.js
│   ├── get-item.js
│   ├── search-history.js
│   ├── get-urls.js
│   ├── get-code.js
│   ├── get-json.js
│   ├── get-secrets-index.js
│   ├── unlock-secret.js
│   ├── copy-item.js
│   ├── pin-item.js
│   ├── tag-item.js
│   ├── delete-item.js
│   ├── clear-history.js
│   ├── get-stats.js
│   └── daemon-status.js
└── cli/
    ├── index.js       ← commander root
    ├── install.js
    ├── uninstall.js
    ├── status.js
    ├── vault.js
    └── doctor.js

bin/
├── clipboard-history          ← shebang, imports src/cli/index.js
└── pasteboard-types           ← compiled from native/pasteboard-types.swift

native/
└── pasteboard-types.swift

scripts/
├── launchd.plist.template
├── build-native.sh
└── refresh-gitleaks-rules.js

vendor/
└── gitleaks.toml

tests/
├── unit/
│   ├── crypto.test.js
│   ├── types.test.js
│   ├── secrets.test.js
│   └── store.test.js
├── integration/
│   └── daemon-mcp.test.js
└── e2e/
    └── full-cycle.test.js

.github/workflows/ci.yml
.github/dependabot.yml
LICENSE                  (MIT)
CHANGELOG.md
CONTRIBUTING.md
README.md (rewritten)
```

---

## Milestones

| # | Range | Outcome |
|---|---|---|
| **M1** | Tasks 1–9 | Foundation: project bootstrap, crypto, types, secrets, store all unit-tested |
| **M2** | Tasks 10–14 | Daemon captures clips into DB; manual `pbcopy` smoke test works |
| **M3** | Tasks 15–22 | MCP server with 15 tools online; Claude can query history |
| **M4** | Tasks 23–28 | CLI with install/uninstall/status/vault/doctor; launchd integration |
| **M5** | Tasks 29–34 | E2E tests, CI, README, polish, release v0.2.0 |

---

# Milestone 1 — Foundation

## Task 1: Bootstrap project structure and dependencies

**Files:**
- Modify: `package.json`
- Create: `tsconfig.json` (used only for type-check, not transpilation)
- Create: `vitest.config.js`
- Create: empty placeholder dirs via `.gitkeep`

- [ ] **Step 1: Update `package.json` with new deps and scripts**

```json
{
  "name": "clipboard-history-mcp",
  "version": "0.2.0-alpha.0",
  "description": "Type-aware, secret-safe macOS clipboard history exposed to Claude via MCP.",
  "type": "module",
  "bin": {
    "clipboard-history": "bin/clipboard-history",
    "clipboard-history-mcp": "src/mcp/index.js"
  },
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest",
    "test:e2e": "vitest run tests/e2e",
    "typecheck": "tsc --noEmit",
    "build:native": "bash scripts/build-native.sh",
    "lint": "eslint src tests",
    "refresh:gitleaks": "node scripts/refresh-gitleaks-rules.js",
    "mcp": "node src/mcp/index.js",
    "daemon": "node src/daemon/index.js"
  },
  "dependencies": {
    "@iarna/toml": "^3.0.0",
    "@modelcontextprotocol/sdk": "^1.0.4",
    "better-sqlite3": "^11.7.0",
    "commander": "^12.1.0",
    "flourite": "^1.2.4"
  },
  "devDependencies": {
    "@types/better-sqlite3": "^7.6.12",
    "@types/node": "^22.0.0",
    "eslint": "^9.0.0",
    "typescript": "^5.6.0",
    "vitest": "^2.1.0"
  },
  "engines": {
    "node": ">=20"
  },
  "files": [
    "bin",
    "src",
    "scripts",
    "vendor",
    "native",
    "README.md",
    "LICENSE",
    "CHANGELOG.md"
  ]
}
```

- [ ] **Step 2: Create `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "allowJs": true,
    "checkJs": true,
    "noEmit": true,
    "strict": false,
    "noImplicitAny": false,
    "esModuleInterop": true,
    "resolveJsonModule": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*.js", "tests/**/*.js"]
}
```

- [ ] **Step 3: Create `vitest.config.js`**

```js
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.js'],
    exclude: ['tests/e2e/**', 'node_modules', 'dist'],
    globals: false,
    testTimeout: 10000,
  },
});
```

- [ ] **Step 4: Install deps and verify**

Run: `npm install`
Expected: deps installed, no errors. `node_modules/better-sqlite3` exists.

- [ ] **Step 5: Commit**

```bash
git add package.json package-lock.json tsconfig.json vitest.config.js
git commit -m "chore: bootstrap v2 deps and tsconfig/vitest config"
```

---

## Task 2: Remove v1 source files

**Files:**
- Delete: `src/index.js`, `src/store.js`, `src/clipboard.js`, `src/watcher.js`

We're rewriting from scratch. Keep `package.json`, `README.md`, `.gitignore`, `node_modules`.

- [ ] **Step 1: Delete v1 src files**

```bash
rm src/index.js src/store.js src/clipboard.js src/watcher.js
```

- [ ] **Step 2: Verify directory empty**

Run: `ls src/`
Expected: empty (or only newly-created subdirs if Task 1 made any)

- [ ] **Step 3: Commit**

```bash
git add -u src/
git commit -m "chore: remove v1 source ahead of v2 rewrite"
```

---

## Task 3: Implement `src/core/crypto.js` (AES-256-GCM)

**Files:**
- Create: `src/core/crypto.js`
- Create: `tests/unit/crypto.test.js`

Encryption is foundational; everything depends on it. Keychain integration is deferred to a later sub-step (the module accepts a key directly so unit tests don't need Keychain).

- [ ] **Step 1: Write failing test**

```js
// tests/unit/crypto.test.js
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
```

- [ ] **Step 2: Run test, verify it fails**

Run: `npx vitest run tests/unit/crypto.test.js`
Expected: FAIL with "Failed to resolve import" or "encrypt is not a function".

- [ ] **Step 3: Implement `src/core/crypto.js`**

```js
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
```

- [ ] **Step 4: Run test, verify it passes**

Run: `npx vitest run tests/unit/crypto.test.js`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/core/crypto.js tests/unit/crypto.test.js
git commit -m "feat(core): AES-256-GCM crypto with Keychain-backed master key"
```

---

## Task 4: Implement `src/core/types.js` (type detection)

**Files:**
- Create: `src/core/types.js`
- Create: `tests/unit/types.test.js`

Note: secret detection lives in `secrets.js` (Task 6). This module integrates with secrets.js by calling it as one classifier among many; we'll wire that in Task 6.

- [ ] **Step 1: Write failing test**

```js
// tests/unit/types.test.js
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
```

- [ ] **Step 2: Run test, verify it fails**

Run: `npx vitest run tests/unit/types.test.js`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `src/core/types.js`**

```js
import detectLanguage from 'flourite';

const URL_RE = /\bhttps?:\/\/[^\s<>"'`]+/i;
const FULL_URL_RE = /^\s*https?:\/\/[^\s<>"'`]+\s*$/i;
const EMAIL_RE = /^\s*[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\s*$/;
const PHONE_RE = /^\s*\+?[\d\s().-]{7,}\s*$/;
const SQL_RE = /^\s*(SELECT|INSERT|UPDATE|DELETE|CREATE|ALTER|DROP|WITH)\b/i;
const SHELL_PREFIXES = /^\s*\$\s|^(sudo|cd|ls|git|npm|pnpm|yarn|brew|docker|kubectl|curl|wget|cargo|go|python|pip|node|make|bash|zsh|sh)\b/;

export function classify(text) {
  if (typeof text !== 'string' || text.length === 0) {
    return { primaryKind: 'text', kinds: ['text'] };
  }

  const kinds = new Set();

  if (FULL_URL_RE.test(text)) kinds.add('url');
  else if (URL_RE.test(text)) kinds.add('url');

  if (EMAIL_RE.test(text)) kinds.add('email');
  if (PHONE_RE.test(text) && /\d/.test(text)) kinds.add('phone');

  if (isJSON(text)) kinds.add('json');
  if (SQL_RE.test(text)) kinds.add('sql');
  if (SHELL_PREFIXES.test(text)) kinds.add('shell');

  let codeLang = null;
  if (text.includes('\n') || /[{};]/.test(text)) {
    const detected = detectLanguage(text, { heuristic: true, shiki: false });
    if (detected?.language && detected.statistics?.[detected.language] > 1) {
      codeLang = detected.language;
      kinds.add(`code:${codeLang.toLowerCase()}`);
    }
  }

  const primaryKind = pickPrimary(kinds, text);
  if (kinds.size === 0) kinds.add('text');

  return {
    primaryKind,
    kinds: [...kinds],
    code: codeLang ? { language: codeLang } : undefined,
  };
}

function isJSON(text) {
  const trimmed = text.trim();
  if (!(trimmed.startsWith('{') || trimmed.startsWith('['))) return false;
  try {
    JSON.parse(trimmed);
    return true;
  } catch {
    return false;
  }
}

function pickPrimary(kinds, text) {
  if (FULL_URL_RE.test(text) && kinds.has('url')) return 'url';
  if (kinds.has('email')) return 'email';
  if (kinds.has('json')) return 'json';
  if (kinds.has('sql')) return 'sql';
  if (kinds.has('shell')) return 'shell';
  for (const k of kinds) if (k.startsWith('code:')) return k;
  if (kinds.has('url')) return 'url';
  if (kinds.has('phone')) return 'phone';
  return 'text';
}
```

- [ ] **Step 4: Run test, verify it passes**

Run: `npx vitest run tests/unit/types.test.js`
Expected: 8 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/core/types.js tests/unit/types.test.js
git commit -m "feat(core): type classifier (url/email/json/sql/shell/code via flourite)"
```

---

## Task 5: Vendor gitleaks rules

**Files:**
- Create: `vendor/gitleaks.toml`
- Create: `scripts/refresh-gitleaks-rules.js`
- Modify: `.gitignore` if needed

- [ ] **Step 1: Download current gitleaks ruleset**

```bash
mkdir -p vendor
curl -fsSL https://raw.githubusercontent.com/gitleaks/gitleaks/master/config/gitleaks.toml -o vendor/gitleaks.toml
wc -l vendor/gitleaks.toml
```

Expected: a TOML file > 2000 lines.

- [ ] **Step 2: Create the refresh script**

```js
// scripts/refresh-gitleaks-rules.js
import { writeFileSync } from 'node:fs';

const URL = 'https://raw.githubusercontent.com/gitleaks/gitleaks/master/config/gitleaks.toml';

const res = await fetch(URL);
if (!res.ok) {
  console.error(`Fetch failed: ${res.status}`);
  process.exit(1);
}
const body = await res.text();
writeFileSync(new URL('../vendor/gitleaks.toml', import.meta.url), body);
console.log(`Wrote ${body.length} bytes to vendor/gitleaks.toml`);
```

- [ ] **Step 3: Add LICENSE attribution stub**

```bash
mkdir -p vendor
```

Create `vendor/README.md`:

```markdown
# Vendored dependencies

## gitleaks.toml

Source: https://github.com/gitleaks/gitleaks (MIT)
Refreshed via `npm run refresh:gitleaks`.
Rule catalog (252+ patterns) is MIT-licensed and reproduced here under MIT terms.
See `LICENSE` at the project root for our license; gitleaks's `LICENSE` is at
https://github.com/gitleaks/gitleaks/blob/master/LICENSE.
```

- [ ] **Step 4: Verify refresh script works**

Run: `node scripts/refresh-gitleaks-rules.js`
Expected: prints "Wrote N bytes...", file size unchanged.

- [ ] **Step 5: Commit**

```bash
git add vendor/ scripts/refresh-gitleaks-rules.js
git commit -m "chore: vendor gitleaks rule catalog (MIT) + refresh script"
```

---

## Task 6: Implement `src/core/secrets.js`

**Files:**
- Create: `src/core/secrets.js`
- Create: `tests/unit/secrets.test.js`

- [ ] **Step 1: Write failing test**

```js
// tests/unit/secrets.test.js
import { describe, it, expect } from 'vitest';
import { detectSecret } from '../../src/core/secrets.js';

describe('detectSecret', () => {
  it('detects OpenAI API keys', () => {
    const r = detectSecret('My key is sk-proj-' + 'A'.repeat(48));
    expect(r).toBeTruthy();
    expect(r.kind).toMatch(/openai/i);
    expect(r.value).toContain('sk-proj-');
    expect(r.lastChars.length).toBeGreaterThanOrEqual(4);
  });

  it('detects AWS access keys', () => {
    const r = detectSecret('AKIAIOSFODNN7EXAMPLE');
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
```

- [ ] **Step 2: Run test, verify it fails**

Run: `npx vitest run tests/unit/secrets.test.js`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `src/core/secrets.js`**

```js
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import TOML from '@iarna/toml';

const RULES_PATH = fileURLToPath(new URL('../../vendor/gitleaks.toml', import.meta.url));

let compiledRules = null;

function loadRules() {
  if (compiledRules) return compiledRules;
  const raw = readFileSync(RULES_PATH, 'utf8');
  const parsed = TOML.parse(raw);
  const ruleSet = (parsed.rules || []).map((r) => {
    try {
      return {
        id: r.id,
        kind: r.id,
        re: new RegExp(r.regex, 'g'),
        keywords: r.keywords || [],
      };
    } catch {
      return null;
    }
  }).filter(Boolean);

  ruleSet.push({
    id: 'jwt',
    kind: 'jwt',
    re: /eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g,
    keywords: [],
  });

  compiledRules = ruleSet;
  return ruleSet;
}

export function detectSecret(text) {
  if (!text) return null;
  const rules = loadRules();
  for (const rule of rules) {
    rule.re.lastIndex = 0;
    const m = rule.re.exec(text);
    if (m) {
      const value = m[0];
      return {
        kind: rule.kind,
        value,
        lastChars: value.slice(-Math.min(6, value.length)),
        startIndex: m.index,
        endIndex: m.index + value.length,
      };
    }
  }

  const ccMatch = text.match(/\b(?:\d[ -]?){13,19}\b/);
  if (ccMatch) {
    const digits = ccMatch[0].replace(/[ -]/g, '');
    if (digits.length >= 13 && digits.length <= 19 && luhnValid(digits)) {
      return {
        kind: 'credit_card',
        value: digits,
        lastChars: digits.slice(-4),
        startIndex: ccMatch.index,
        endIndex: ccMatch.index + ccMatch[0].length,
      };
    }
  }

  return null;
}

function luhnValid(digits) {
  let sum = 0;
  let alt = false;
  for (let i = digits.length - 1; i >= 0; i--) {
    let n = parseInt(digits[i], 10);
    if (alt) {
      n *= 2;
      if (n > 9) n -= 9;
    }
    sum += n;
    alt = !alt;
  }
  return sum % 10 === 0;
}
```

- [ ] **Step 4: Run test, verify it passes**

Run: `npx vitest run tests/unit/secrets.test.js`
Expected: 6 tests pass.

If gitleaks rule for OpenAI doesn't match the test fixture, adjust the fixture to a known-matching shape (e.g. `sk-proj-` + 48 chars + a digit) by reading the relevant rule in `vendor/gitleaks.toml` and copying its expected format.

- [ ] **Step 5: Commit**

```bash
git add src/core/secrets.js tests/unit/secrets.test.js
git commit -m "feat(core): secret detector backed by gitleaks rules + JWT + Luhn"
```

---

## Task 7: Implement `src/core/db.js` (SQLite + FTS5 + migrations)

**Files:**
- Create: `src/core/db.js`
- Create: `tests/unit/store.test.js` (initial — store.js comes in Task 8)

We expose a single `openDb(path)` function that creates the file if missing, runs migrations, returns a `better-sqlite3` Database instance with FTS5 ready.

- [ ] **Step 1: Write failing test for fresh DB schema**

```js
// tests/unit/store.test.js (will hold both db and store tests)
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { openDb } from '../../src/core/db.js';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { rmSync, mkdtempSync } from 'node:fs';

describe('db: schema and migrations', () => {
  let tmpDir, dbPath, db;
  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'cbhist-'));
    dbPath = join(tmpDir, 'test.db');
    db = openDb(dbPath);
  });
  afterEach(() => {
    db.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('creates clips, secrets, kinds, tags, meta tables', () => {
    const tables = db
      .prepare(`SELECT name FROM sqlite_master WHERE type='table' ORDER BY name`)
      .all()
      .map((r) => r.name);
    expect(tables).toEqual(
      expect.arrayContaining(['clips', 'secrets', 'kinds', 'tags', 'meta'])
    );
  });

  it('creates an FTS5 virtual table', () => {
    const fts = db
      .prepare(`SELECT name FROM sqlite_master WHERE type='table' AND name='clips_fts'`)
      .get();
    expect(fts).toBeTruthy();
  });

  it('records the schema version in meta', () => {
    const v = db.prepare(`SELECT value FROM meta WHERE key='schema_version'`).get();
    expect(parseInt(v.value, 10)).toBeGreaterThanOrEqual(1);
  });

  it('enables WAL journal mode', () => {
    const mode = db.pragma('journal_mode', { simple: true });
    expect(mode).toBe('wal');
  });
});
```

- [ ] **Step 2: Run test, verify it fails**

Run: `npx vitest run tests/unit/store.test.js`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `src/core/db.js`**

```js
import Database from 'better-sqlite3';
import { dirname } from 'node:path';
import { mkdirSync } from 'node:fs';

const MIGRATIONS = [
  function v1(db) {
    db.exec(`
      CREATE TABLE clips (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        uuid TEXT NOT NULL UNIQUE,
        text TEXT,
        preview TEXT NOT NULL,
        length INTEGER NOT NULL,
        byte_length INTEGER NOT NULL,
        hash TEXT NOT NULL,
        primary_kind TEXT NOT NULL,
        source_app TEXT,
        window_title TEXT,
        first_copied_at INTEGER NOT NULL,
        last_copied_at INTEGER NOT NULL,
        copy_count INTEGER NOT NULL DEFAULT 1,
        paste_count INTEGER NOT NULL DEFAULT 0,
        is_pinned INTEGER NOT NULL DEFAULT 0
      );
      CREATE INDEX idx_clips_hash ON clips(hash);
      CREATE INDEX idx_clips_kind ON clips(primary_kind);
      CREATE INDEX idx_clips_last_copied ON clips(last_copied_at DESC);

      CREATE TABLE kinds (
        clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        PRIMARY KEY (clip_id, kind)
      );

      CREATE TABLE tags (
        clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
        tag TEXT NOT NULL,
        PRIMARY KEY (clip_id, tag)
      );

      CREATE TABLE secrets (
        clip_id INTEGER PRIMARY KEY REFERENCES clips(id) ON DELETE CASCADE,
        ciphertext BLOB,
        nonce BLOB,
        secret_kind TEXT NOT NULL,
        last_chars TEXT NOT NULL,
        unlock_count INTEGER NOT NULL DEFAULT 0
      );

      CREATE VIRTUAL TABLE clips_fts USING fts5(
        preview, window_title, primary_kind,
        content='clips', content_rowid='id', tokenize='porter unicode61'
      );

      CREATE TRIGGER clips_ai AFTER INSERT ON clips BEGIN
        INSERT INTO clips_fts(rowid, preview, window_title, primary_kind)
        VALUES (new.id, new.preview, coalesce(new.window_title, ''), new.primary_kind);
      END;

      CREATE TRIGGER clips_ad AFTER DELETE ON clips BEGIN
        INSERT INTO clips_fts(clips_fts, rowid, preview, window_title, primary_kind)
        VALUES ('delete', old.id, old.preview, coalesce(old.window_title, ''), old.primary_kind);
      END;

      CREATE TRIGGER clips_au AFTER UPDATE ON clips BEGIN
        INSERT INTO clips_fts(clips_fts, rowid, preview, window_title, primary_kind)
        VALUES ('delete', old.id, old.preview, coalesce(old.window_title, ''), old.primary_kind);
        INSERT INTO clips_fts(rowid, preview, window_title, primary_kind)
        VALUES (new.id, new.preview, coalesce(new.window_title, ''), new.primary_kind);
      END;

      CREATE TABLE meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );
    `);
    db.prepare(`INSERT INTO meta(key, value) VALUES (?, ?)`).run('schema_version', '1');
  },
];

export function openDb(path) {
  mkdirSync(dirname(path), { recursive: true });
  const db = new Database(path);
  db.pragma('journal_mode = WAL');
  db.pragma('foreign_keys = ON');
  db.pragma('synchronous = NORMAL');

  let current = 0;
  const meta = db.prepare(
    `SELECT value FROM meta WHERE key='schema_version'`
  );
  try {
    const row = meta.get();
    if (row) current = parseInt(row.value, 10);
  } catch {
    current = 0;
  }

  for (let i = current; i < MIGRATIONS.length; i++) {
    const fn = MIGRATIONS[i];
    db.transaction(fn)(db);
  }

  return db;
}
```

- [ ] **Step 4: Run test, verify it passes**

Run: `npx vitest run tests/unit/store.test.js`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/core/db.js tests/unit/store.test.js
git commit -m "feat(core): SQLite + FTS5 schema with WAL and migrations"
```

---

## Task 8: Implement `src/core/store.js` (high-level CRUD)

**Files:**
- Create: `src/core/store.js`
- Modify: `tests/unit/store.test.js` (add store tests)

- [ ] **Step 1: Add store tests to existing file**

Append to `tests/unit/store.test.js`:

```js
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { Store } from '../../src/core/store.js';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { randomBytes } from 'node:crypto';

describe('Store', () => {
  let tmpDir, store;
  const key = randomBytes(32);

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'cbhist-store-'));
    store = new Store(join(tmpDir, 'test.db'), { masterKey: key });
  });
  afterEach(() => {
    store.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('inserts a clean clip', () => {
    const id = store.addClip({
      text: 'hello world',
      primaryKind: 'text',
      kinds: ['text'],
      sourceApp: 'Terminal',
    });
    expect(id).toBeGreaterThan(0);
    const item = store.getItem(id);
    expect(item.text).toBe('hello world');
    expect(item.primaryKind).toBe('text');
    expect(item.sourceApp).toBe('Terminal');
  });

  it('dedups identical text by hash', () => {
    const id1 = store.addClip({ text: 'same', primaryKind: 'text', kinds: ['text'] });
    const id2 = store.addClip({ text: 'same', primaryKind: 'text', kinds: ['text'] });
    expect(id2).toBe(id1);
    const item = store.getItem(id1);
    expect(item.copyCount).toBe(2);
  });

  it('stores secret with encryption and redacted preview', () => {
    const id = store.addSecret({
      text: 'sk-proj-XYZ',
      secretKind: 'openai_api_key',
      sourceApp: 'Safari',
      windowTitle: 'OpenAI Platform — API Keys',
    });
    const item = store.getItem(id);
    expect(item.text).toBeNull();
    expect(item.preview).toMatch(/REDACTED/);
    expect(item.windowTitle).toBe('OpenAI Platform — API Keys');
    expect(store.unlockSecret(id)).toBe('sk-proj-XYZ');
  });

  it('FTS search hits secret by window title', () => {
    store.addSecret({
      text: 'sk-proj-XYZ',
      secretKind: 'openai_api_key',
      sourceApp: 'Safari',
      windowTitle: 'OpenAI Platform — API Keys',
    });
    const results = store.search({ query: 'OpenAI' });
    expect(results.length).toBeGreaterThan(0);
    expect(results[0].text).toBeNull();
  });

  it('lists with kind filter', () => {
    store.addClip({ text: 'https://a.com', primaryKind: 'url', kinds: ['url'] });
    store.addClip({ text: 'plain', primaryKind: 'text', kinds: ['text'] });
    const onlyUrls = store.list({ kind: 'url' });
    expect(onlyUrls.length).toBe(1);
    expect(onlyUrls[0].primaryKind).toBe('url');
  });

  it('clear scopes', () => {
    store.addClip({ text: 'a', primaryKind: 'text', kinds: ['text'] });
    store.addClip({ text: 'b', primaryKind: 'text', kinds: ['text'] });
    expect(store.clear({ scope: 'all' })).toBe(2);
    expect(store.list({}).length).toBe(0);
  });
});
```

- [ ] **Step 2: Run test, verify it fails**

Run: `npx vitest run tests/unit/store.test.js`
Expected: FAIL — `Store` not exported from `store.js`.

- [ ] **Step 3: Implement `src/core/store.js`**

```js
import { createHash, randomUUID } from 'node:crypto';
import { openDb } from './db.js';
import { encrypt, decrypt } from './crypto.js';

export class Store {
  constructor(path, { masterKey } = {}) {
    this.db = openDb(path);
    this.masterKey = masterKey;
    this.prepared = {
      insertClip: this.db.prepare(`
        INSERT INTO clips
          (uuid, text, preview, length, byte_length, hash, primary_kind,
           source_app, window_title, first_copied_at, last_copied_at)
        VALUES
          (@uuid, @text, @preview, @length, @byteLength, @hash, @primaryKind,
           @sourceApp, @windowTitle, @now, @now)
      `),
      bumpDuplicate: this.db.prepare(`
        UPDATE clips SET copy_count = copy_count + 1, last_copied_at = ?
        WHERE id = ?
      `),
      findHash: this.db.prepare(`SELECT id FROM clips WHERE hash = ? LIMIT 1`),
      insertKind: this.db.prepare(`INSERT OR IGNORE INTO kinds(clip_id, kind) VALUES (?, ?)`),
      insertTag: this.db.prepare(`INSERT OR IGNORE INTO tags(clip_id, tag) VALUES (?, ?)`),
      removeTag: this.db.prepare(`DELETE FROM tags WHERE clip_id = ? AND tag = ?`),
      insertSecret: this.db.prepare(`
        INSERT INTO secrets(clip_id, ciphertext, nonce, secret_kind, last_chars)
        VALUES (?, ?, ?, ?, ?)
      `),
      getSecret: this.db.prepare(`SELECT * FROM secrets WHERE clip_id = ?`),
      bumpUnlock: this.db.prepare(
        `UPDATE secrets SET unlock_count = unlock_count + 1 WHERE clip_id = ?`
      ),
      getById: this.db.prepare(`SELECT * FROM clips WHERE id = ?`),
      getKinds: this.db.prepare(`SELECT kind FROM kinds WHERE clip_id = ?`),
      getTags: this.db.prepare(`SELECT tag FROM tags WHERE clip_id = ?`),
      pin: this.db.prepare(`UPDATE clips SET is_pinned = ? WHERE id = ?`),
      bumpPaste: this.db.prepare(
        `UPDATE clips SET paste_count = paste_count + 1 WHERE id = ?`
      ),
      remove: this.db.prepare(`DELETE FROM clips WHERE id = ?`),
    };
  }

  close() {
    this.db.close();
  }

  addClip({ text, primaryKind, kinds = [], sourceApp = null, windowTitle = null }) {
    const hash = sha256(text);
    const existing = this.prepared.findHash.get(hash);
    const now = Date.now();
    if (existing) {
      this.prepared.bumpDuplicate.run(now, existing.id);
      return existing.id;
    }
    const insert = this.db.transaction(() => {
      const r = this.prepared.insertClip.run({
        uuid: randomUUID(),
        text,
        preview: text.slice(0, 200),
        length: text.length,
        byteLength: Buffer.byteLength(text, 'utf8'),
        hash,
        primaryKind,
        sourceApp,
        windowTitle,
        now,
      });
      const id = Number(r.lastInsertRowid);
      for (const k of new Set(kinds.length ? kinds : [primaryKind])) {
        this.prepared.insertKind.run(id, k);
      }
      return id;
    });
    return insert();
  }

  addSecret({ text, secretKind, sourceApp = null, windowTitle = null }) {
    if (!this.masterKey) throw new Error('master key not provided');
    const lastChars = text.slice(-Math.min(6, text.length));
    const preview = `[REDACTED:${secretKind}]`;
    const hash = sha256(text);
    const now = Date.now();
    const insert = this.db.transaction(() => {
      const r = this.prepared.insertClip.run({
        uuid: randomUUID(),
        text: null,
        preview,
        length: text.length,
        byteLength: Buffer.byteLength(text, 'utf8'),
        hash,
        primaryKind: `secret:${secretKind}`,
        sourceApp,
        windowTitle,
        now,
      });
      const id = Number(r.lastInsertRowid);
      this.prepared.insertKind.run(id, `secret:${secretKind}`);
      const { ciphertext, nonce } = encrypt(text, this.masterKey);
      this.prepared.insertSecret.run(id, ciphertext, nonce, secretKind, lastChars);
      return id;
    });
    return insert();
  }

  getItem(id) {
    const row = this.prepared.getById.get(id);
    if (!row) return null;
    const kinds = this.prepared.getKinds.all(id).map((r) => r.kind);
    const tags = this.prepared.getTags.all(id).map((r) => r.tag);
    return rowToItem(row, kinds, tags);
  }

  unlockSecret(id) {
    const row = this.prepared.getSecret.get(id);
    if (!row) throw new Error(`No secret for clip ${id}`);
    if (!row.ciphertext) throw new Error('Secret stored with NEVER_STORE_SECRETS=1');
    if (!this.masterKey) throw new Error('master key not provided');
    const value = decrypt(row.ciphertext, row.nonce, this.masterKey);
    this.prepared.bumpUnlock.run(id);
    return value;
  }

  list({ limit = 20, offset = 0, kind = null, sourceApp = null, since = null, pinnedOnly = false } = {}) {
    const where = [];
    const params = {};
    if (kind) {
      where.push(
        `id IN (SELECT clip_id FROM kinds WHERE kind = @kind)`
      );
      params.kind = kind;
    }
    if (sourceApp) { where.push(`source_app = @sourceApp`); params.sourceApp = sourceApp; }
    if (since) { where.push(`last_copied_at >= @since`); params.since = since; }
    if (pinnedOnly) where.push(`is_pinned = 1`);
    const sql = `
      SELECT * FROM clips
      ${where.length ? 'WHERE ' + where.join(' AND ') : ''}
      ORDER BY is_pinned DESC, last_copied_at DESC
      LIMIT @limit OFFSET @offset
    `;
    params.limit = limit;
    params.offset = offset;
    const rows = this.db.prepare(sql).all(params);
    return rows.map((r) => {
      const kinds = this.prepared.getKinds.all(r.id).map((x) => x.kind);
      const tags = this.prepared.getTags.all(r.id).map((x) => x.tag);
      return rowToItem(r, kinds, tags);
    });
  }

  search({ query, limit = 20 }) {
    const ftsQuery = sanitizeFts(query);
    const rows = this.db.prepare(`
      SELECT clips.*, bm25(clips_fts) AS rank
      FROM clips_fts
      JOIN clips ON clips.id = clips_fts.rowid
      WHERE clips_fts MATCH ?
      ORDER BY rank
      LIMIT ?
    `).all(ftsQuery, limit);
    return rows.map((r) => {
      const kinds = this.prepared.getKinds.all(r.id).map((x) => x.kind);
      const tags = this.prepared.getTags.all(r.id).map((x) => x.tag);
      return rowToItem(r, kinds, tags);
    });
  }

  pin(id, value = true) { this.prepared.pin.run(value ? 1 : 0, id); }
  unpin(id) { this.pin(id, false); }
  tag(id, tag) { this.prepared.insertTag.run(id, tag); }
  untag(id, tag) { this.prepared.removeTag.run(id, tag); }
  bumpPaste(id) { this.prepared.bumpPaste.run(id); }
  delete(id) { this.prepared.remove.run(id); }

  clear({ scope = 'all' } = {}) {
    if (scope === 'all') {
      const r = this.db.prepare(`DELETE FROM clips`).run();
      return r.changes;
    }
    if (scope.startsWith('older_than_days:')) {
      const days = parseInt(scope.split(':')[1], 10);
      const cutoff = Date.now() - days * 86_400_000;
      return this.db.prepare(`DELETE FROM clips WHERE last_copied_at < ?`).run(cutoff).changes;
    }
    if (scope.startsWith('kind:')) {
      const k = scope.split(':')[1];
      return this.db.prepare(
        `DELETE FROM clips WHERE primary_kind = ? OR id IN (SELECT clip_id FROM kinds WHERE kind = ?)`
      ).run(k, k).changes;
    }
    throw new Error(`Unknown clear scope: ${scope}`);
  }

  stats() {
    const total = this.db.prepare(`SELECT COUNT(*) AS n FROM clips`).get().n;
    const oldest = this.db.prepare(`SELECT MIN(first_copied_at) AS t FROM clips`).get().t;
    const newest = this.db.prepare(`SELECT MAX(last_copied_at) AS t FROM clips`).get().t;
    const byKind = this.db.prepare(
      `SELECT primary_kind AS kind, COUNT(*) AS n FROM clips GROUP BY primary_kind`
    ).all();
    return { count: total, oldest, newest, byKind };
  }
}

function sha256(text) {
  return createHash('sha256').update(text).digest('hex');
}

function rowToItem(row, kinds, tags) {
  return {
    id: row.id,
    uuid: row.uuid,
    text: row.text,
    preview: row.preview,
    length: row.length,
    primaryKind: row.primary_kind,
    kinds,
    tags,
    sourceApp: row.source_app,
    windowTitle: row.window_title,
    firstCopiedAt: row.first_copied_at,
    lastCopiedAt: row.last_copied_at,
    copyCount: row.copy_count,
    pasteCount: row.paste_count,
    isPinned: row.is_pinned === 1,
  };
}

function sanitizeFts(q) {
  return q.replace(/["*()]/g, ' ').trim().split(/\s+/).filter(Boolean).join(' ');
}
```

- [ ] **Step 4: Run test, verify it passes**

Run: `npx vitest run tests/unit/store.test.js`
Expected: 10 tests pass (4 db + 6 store).

- [ ] **Step 5: Commit**

```bash
git add src/core/store.js tests/unit/store.test.js
git commit -m "feat(core): Store class with dedup, secrets vault, FTS5 search"
```

---

## Task 9: Build Swift helper for NSPasteboard types

**Files:**
- Create: `native/pasteboard-types.swift`
- Create: `scripts/build-native.sh`
- (Generated) `bin/pasteboard-types`

- [ ] **Step 1: Write the Swift helper**

```swift
// native/pasteboard-types.swift
import AppKit

let pb = NSPasteboard.general
let types = pb.types?.map { $0.rawValue } ?? []
let json = "{\"types\":[" + types.map { "\"\($0)\"" }.joined(separator: ",") + "]}"
print(json)
```

- [ ] **Step 2: Write the build script**

```bash
#!/usr/bin/env bash
# scripts/build-native.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$HERE/bin"
swiftc -O "$HERE/native/pasteboard-types.swift" -o "$HERE/bin/pasteboard-types"
echo "Built $HERE/bin/pasteboard-types"
```

- [ ] **Step 3: Make executable and run**

```bash
chmod +x scripts/build-native.sh
bash scripts/build-native.sh
./bin/pasteboard-types
```

Expected: prints JSON like `{"types":["public.utf8-plain-text",...]}`.

- [ ] **Step 4: Add `bin/pasteboard-types` to `.gitignore`**

Add line to `.gitignore`:

```
/bin/pasteboard-types
```

- [ ] **Step 5: Commit**

```bash
git add native/ scripts/build-native.sh .gitignore
git commit -m "feat(native): Swift helper enumerating NSPasteboard types"
```

---

# Milestone 2 — Daemon

## Task 10: Implement `src/core/pasteboard.js`

**Files:**
- Create: `src/core/pasteboard.js`

- [ ] **Step 1: Implement**

```js
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const TYPES_BIN = resolve(HERE, '../../bin/pasteboard-types');

const TRANSIENT_TYPES = new Set([
  'org.nspasteboard.TransientType',
  'org.nspasteboard.ConcealedType',
  'org.nspasteboard.AutoGeneratedType',
  'com.apple.is-sensitive',
]);

export function readClipboard() {
  return new Promise((resolve, reject) => {
    const p = spawn('pbpaste', [], { stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '', err = '';
    p.stdout.on('data', (c) => out += c.toString('utf8'));
    p.stderr.on('data', (c) => err += c.toString('utf8'));
    p.on('error', reject);
    p.on('close', (code) =>
      code === 0 ? resolve(out) : reject(new Error(`pbpaste ${code}: ${err}`))
    );
  });
}

export function writeClipboard(text) {
  return new Promise((resolve, reject) => {
    const p = spawn('pbcopy', [], { stdio: ['pipe', 'ignore', 'pipe'] });
    p.on('error', reject);
    p.on('close', (code) =>
      code === 0 ? resolve() : reject(new Error(`pbcopy ${code}`))
    );
    p.stdin.end(text, 'utf8');
  });
}

export function isTransient() {
  const r = spawnSync(TYPES_BIN, [], { encoding: 'utf8' });
  if (r.status !== 0) return false;
  try {
    const { types } = JSON.parse(r.stdout);
    return types.some((t) => TRANSIENT_TYPES.has(t));
  } catch {
    return false;
  }
}
```

- [ ] **Step 2: Smoke test manually**

Run: `pbcopy <<< "test"; node -e "import('./src/core/pasteboard.js').then(m => m.readClipboard().then(t => console.log('got:', t.trim())))"`
Expected: prints `got: test`.

- [ ] **Step 3: Commit**

```bash
git add src/core/pasteboard.js
git commit -m "feat(core): pasteboard wrappers + transient-type guard"
```

---

## Task 11: Implement `src/daemon/context.js`

**Files:**
- Create: `src/daemon/context.js`

- [ ] **Step 1: Implement**

```js
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
```

- [ ] **Step 2: Smoke test**

Run: `node -e "import('./src/daemon/context.js').then(m => console.log(m.captureContext()))"`
Expected: `{ frontApp: 'Terminal' or similar, windowTitle: null }`.

- [ ] **Step 3: Commit**

```bash
git add src/daemon/context.js
git commit -m "feat(daemon): frontmost-app and window-title capture via osascript"
```

---

## Task 12: Implement `src/daemon/watcher.js`

**Files:**
- Create: `src/daemon/watcher.js`

- [ ] **Step 1: Implement**

```js
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
```

- [ ] **Step 2: Commit (no test yet — covered in integration test)**

```bash
git add src/daemon/watcher.js
git commit -m "feat(daemon): watcher loop wiring pasteboard + types + secrets + store"
```

---

## Task 13: Implement `src/daemon/index.js` (entry)

**Files:**
- Create: `src/daemon/index.js`

- [ ] **Step 1: Implement**

```js
#!/usr/bin/env node
import { writeFileSync, unlinkSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { mkdirSync } from 'node:fs';

import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';
import { startWatcher } from './watcher.js';

const DEFAULT_DIR = process.env.CLIPBOARD_DATA_DIR
  || join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const DEFAULT_DB = process.env.CLIPBOARD_DB_PATH || join(DEFAULT_DIR, 'history.db');
const PID_FILE = join(DEFAULT_DIR, 'daemon.pid');

mkdirSync(DEFAULT_DIR, { recursive: true });

const masterKey = getOrCreateMasterKey();
const store = new Store(DEFAULT_DB, { masterKey });

writeFileSync(PID_FILE, String(process.pid));

const stop = startWatcher(store, {
  intervalMs: Number(process.env.CLIPBOARD_POLL_MS) || 1500,
  captureWindowTitle: process.env.CLIPBOARD_CAPTURE_WINDOW_TITLE === '1',
  ignoreApps: (process.env.CLIPBOARD_IGNORE_APPS || '').split(',').map(s => s.trim()).filter(Boolean),
  neverStoreSecrets: process.env.CLIPBOARD_NEVER_STORE_SECRETS === '1',
  log: (m) => process.stdout.write(`[daemon] ${new Date().toISOString()} ${m}\n`),
  onError: (e) => process.stderr.write(`[daemon] ${new Date().toISOString()} ERROR ${e.message}\n`),
});

const shutdown = () => {
  stop();
  store.close();
  if (existsSync(PID_FILE)) unlinkSync(PID_FILE);
  process.exit(0);
};
process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);

process.stdout.write(`[daemon] started pid=${process.pid} db=${DEFAULT_DB}\n`);
```

- [ ] **Step 2: Smoke test**

Run in one terminal: `node src/daemon/index.js`
In another: `pbcopy <<< "test clip"`
Wait 2 sec, then check the daemon log line: `clip captured: text from <app>`.
Stop with Ctrl-C.

- [ ] **Step 3: Verify DB written**

Run: `sqlite3 ~/Library/Application\ Support/clipboard-history-mcp/history.db "SELECT id, primary_kind, source_app, preview FROM clips;"`
Expected: at least one row with text "test clip".

- [ ] **Step 4: Commit**

```bash
git add src/daemon/index.js
git commit -m "feat(daemon): entry, lifecycle, master-key bootstrap, PID file"
```

---

## Task 14: Add transient-type smoke test

**Files:**
- Create: `tests/integration/daemon-mcp.test.js` (initial)

- [ ] **Step 1: Write a basic integration test**

```js
// tests/integration/daemon-mcp.test.js
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Store } from '../../src/core/store.js';
import { startWatcher } from '../../src/daemon/watcher.js';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';

describe('daemon: end-to-end capture into Store', () => {
  let tmpDir, store, stop;
  beforeAll(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'cbhist-int-'));
    store = new Store(join(tmpDir, 'test.db'), { masterKey: randomBytes(32) });
    stop = startWatcher(store, { intervalMs: 250, log: () => {} });
  });
  afterAll(() => {
    stop();
    store.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('captures a copied URL', async () => {
    const url = `https://test.example/${Date.now()}`;
    spawnSync('pbcopy', [], { input: url });
    await new Promise((r) => setTimeout(r, 600));
    const items = store.list({ kind: 'url' });
    expect(items.some((i) => i.text === url)).toBe(true);
  });
});
```

- [ ] **Step 2: Run integration test**

Run: `npx vitest run tests/integration/daemon-mcp.test.js`
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/daemon-mcp.test.js
git commit -m "test(integration): watcher captures pasteboard into store"
```

---

# Milestone 3 — MCP server

## Task 15: Build tool registry skeleton

**Files:**
- Create: `src/tools/index.js`

- [ ] **Step 1: Implement registry**

```js
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
```

- [ ] **Step 2: Commit**

```bash
git add src/tools/index.js
git commit -m "feat(tools): registry skeleton for MCP tool dispatch"
```

---

## Task 16: Implement read tools (list, get, search, get_urls, get_code, get_json)

**Files:**
- Create: `src/tools/list-history.js`
- Create: `src/tools/get-item.js`
- Create: `src/tools/search-history.js`
- Create: `src/tools/get-urls.js`
- Create: `src/tools/get-code.js`
- Create: `src/tools/get-json.js`

Each follows the same pattern. Showing all six in full to avoid the "similar to" anti-pattern.

- [ ] **Step 1: `src/tools/list-history.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'list_history',
  description: 'List recent clipboard entries, newest first. Secret values are never returned — for those, only metadata.',
  inputSchema: {
    type: 'object',
    properties: {
      limit: { type: 'number', minimum: 1, maximum: 200 },
      offset: { type: 'number', minimum: 0 },
      kind: { type: 'string', description: 'filter by primary or secondary kind' },
      source_app: { type: 'string' },
      since: { type: 'number', description: 'epoch ms; only items copied after this' },
      pinned_only: { type: 'boolean' },
    },
  },
  handler: ({ limit, offset, kind, source_app, since, pinned_only }, { store }) => {
    const items = store.list({ limit, offset, kind, sourceApp: source_app, since, pinnedOnly: pinned_only });
    return { count: items.length, items };
  },
});
```

- [ ] **Step 2: `src/tools/get-item.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'get_item',
  description: 'Fetch one clipboard entry by id. For secrets returns metadata only — call unlock_secret to decrypt.',
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'number' } },
    required: ['id'],
  },
  handler: ({ id }, { store }) => {
    const item = store.getItem(Number(id));
    if (!item) return { error: `not found: ${id}` };
    if (item.primaryKind?.startsWith('secret:')) {
      return { ...item, requiresUnlock: true };
    }
    return item;
  },
});
```

- [ ] **Step 3: `src/tools/search-history.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'search_history',
  description: 'Full-text search clipboard history (FTS5 BM25). Searches across preview, window title, and primary kind. Secret values still hidden.',
  inputSchema: {
    type: 'object',
    properties: {
      query: { type: 'string' },
      limit: { type: 'number', minimum: 1, maximum: 200 },
    },
    required: ['query'],
  },
  handler: ({ query, limit }, { store }) => {
    const items = store.search({ query, limit: limit ?? 20 });
    return { count: items.length, items };
  },
});
```

- [ ] **Step 4: `src/tools/get-urls.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'get_urls',
  description: 'Return URL clips, deduped by hostname, ranked by paste_count + recency.',
  inputSchema: {
    type: 'object',
    properties: {
      limit: { type: 'number', minimum: 1, maximum: 100 },
      since: { type: 'number' },
    },
  },
  handler: ({ limit = 20, since }, { store }) => {
    const items = store.list({ kind: 'url', limit: 200, since });
    const dedup = new Map();
    for (const it of items) {
      try {
        const host = new URL(it.text).hostname;
        const existing = dedup.get(host);
        if (!existing || it.lastCopiedAt > existing.lastCopiedAt) dedup.set(host, it);
      } catch { /* skip invalid url */ }
    }
    return { count: dedup.size, items: [...dedup.values()].slice(0, limit) };
  },
});
```

- [ ] **Step 5: `src/tools/get-code.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'get_code',
  description: 'Return code clips, optionally filtered by language (js, python, go, rust, ...).',
  inputSchema: {
    type: 'object',
    properties: {
      language: { type: 'string' },
      limit: { type: 'number', minimum: 1, maximum: 100 },
    },
  },
  handler: ({ language, limit = 20 }, { store }) => {
    const kindFilter = language ? `code:${language.toLowerCase()}` : null;
    const items = kindFilter
      ? store.list({ kind: kindFilter, limit })
      : store.list({ limit }).filter((i) => i.primaryKind.startsWith('code:'));
    return { count: items.length, items };
  },
});
```

- [ ] **Step 6: `src/tools/get-json.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'get_json',
  description: 'Return JSON clips with parsed structure preview.',
  inputSchema: {
    type: 'object',
    properties: { limit: { type: 'number', minimum: 1, maximum: 100 } },
  },
  handler: ({ limit = 20 }, { store }) => {
    const items = store.list({ kind: 'json', limit });
    return {
      count: items.length,
      items: items.map((i) => {
        let parsed = null;
        try { parsed = JSON.parse(i.text); } catch { /* keep null */ }
        return { ...i, parsed };
      }),
    };
  },
});
```

- [ ] **Step 7: Commit**

```bash
git add src/tools/list-history.js src/tools/get-item.js src/tools/search-history.js src/tools/get-urls.js src/tools/get-code.js src/tools/get-json.js
git commit -m "feat(tools): read tools (list, get, search, urls, code, json)"
```

---

## Task 17: Implement secret tools (`get_secrets_index`, `unlock_secret`)

**Files:**
- Create: `src/tools/get-secrets-index.js`
- Create: `src/tools/unlock-secret.js`

- [ ] **Step 1: `src/tools/get-secrets-index.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'get_secrets_index',
  description: 'List secret clips with metadata only — no values. Use unlock_secret(id, reason) to retrieve a value.',
  inputSchema: {
    type: 'object',
    properties: { kind: { type: 'string', description: 'filter by secret kind, e.g. openai_api_key' } },
  },
  handler: ({ kind }, { store }) => {
    const filter = kind ? `secret:${kind}` : null;
    const items = filter
      ? store.list({ kind: filter, limit: 200 })
      : store.list({ limit: 500 }).filter((i) => i.primaryKind.startsWith('secret:'));
    return {
      count: items.length,
      items: items.map((i) => ({
        id: i.id,
        secretKind: i.primaryKind.replace(/^secret:/, ''),
        sourceApp: i.sourceApp,
        windowTitle: i.windowTitle,
        firstCopiedAt: i.firstCopiedAt,
        lastCopiedAt: i.lastCopiedAt,
      })),
    };
  },
});
```

- [ ] **Step 2: `src/tools/unlock-secret.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'unlock_secret',
  description: 'Decrypt and return a stored secret. The reason argument is mandatory and surfaces in tool-call logs.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'number' },
      reason: { type: 'string', minLength: 3 },
    },
    required: ['id', 'reason'],
  },
  handler: ({ id, reason }, { store, log }) => {
    log?.(`unlock_secret id=${id} reason="${reason}"`);
    const value = store.unlockSecret(Number(id));
    const item = store.getItem(Number(id));
    return { value, kind: item.primaryKind, lastChars: value.slice(-Math.min(6, value.length)) };
  },
});
```

- [ ] **Step 3: Commit**

```bash
git add src/tools/get-secrets-index.js src/tools/unlock-secret.js
git commit -m "feat(tools): get_secrets_index + unlock_secret with reason audit"
```

---

## Task 18: Implement write tools (`copy_item`, `pin_item`, `tag_item`, `delete_item`, `clear_history`)

**Files:**
- Create: `src/tools/copy-item.js`
- Create: `src/tools/pin-item.js`
- Create: `src/tools/tag-item.js`
- Create: `src/tools/delete-item.js`
- Create: `src/tools/clear-history.js`

- [ ] **Step 1: `src/tools/copy-item.js`**

```js
import { defineTool } from './index.js';
import { writeClipboard } from '../core/pasteboard.js';

defineTool({
  name: 'copy_item',
  description: 'Restore a stored clip to the system clipboard so the user can paste it. For secrets, fails — use unlock_secret first.',
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'number' } },
    required: ['id'],
  },
  handler: async ({ id }, { store }) => {
    const item = store.getItem(Number(id));
    if (!item) return { error: `not found: ${id}` };
    if (!item.text) return { error: 'cannot restore secret directly; use unlock_secret', requiresUnlock: true };
    await writeClipboard(item.text);
    store.bumpPaste(Number(id));
    return { ok: true, id, length: item.length };
  },
});
```

- [ ] **Step 2: `src/tools/pin-item.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'pin_item',
  description: 'Pin or unpin a clip so it stays at the top of list_history.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'number' },
      pinned: { type: 'boolean' },
    },
    required: ['id', 'pinned'],
  },
  handler: ({ id, pinned }, { store }) => {
    if (pinned) store.pin(Number(id));
    else store.unpin(Number(id));
    return { ok: true };
  },
});
```

- [ ] **Step 3: `src/tools/tag-item.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'tag_item',
  description: 'Add or remove a user tag on a clip.',
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'number' },
      tag: { type: 'string', minLength: 1 },
      remove: { type: 'boolean' },
    },
    required: ['id', 'tag'],
  },
  handler: ({ id, tag, remove }, { store }) => {
    if (remove) store.untag(Number(id), tag);
    else store.tag(Number(id), tag);
    return { ok: true };
  },
});
```

- [ ] **Step 4: `src/tools/delete-item.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'delete_item',
  description: 'Hard-delete a clip and (if secret) its encrypted blob.',
  inputSchema: {
    type: 'object',
    properties: { id: { type: 'number' } },
    required: ['id'],
  },
  handler: ({ id }, { store }) => {
    store.delete(Number(id));
    return { ok: true };
  },
});
```

- [ ] **Step 5: `src/tools/clear-history.js`**

```js
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
```

- [ ] **Step 6: Commit**

```bash
git add src/tools/copy-item.js src/tools/pin-item.js src/tools/tag-item.js src/tools/delete-item.js src/tools/clear-history.js
git commit -m "feat(tools): write tools (copy, pin, tag, delete, clear)"
```

---

## Task 19: Implement system tools (`get_stats`, `daemon_status`)

**Files:**
- Create: `src/tools/get-stats.js`
- Create: `src/tools/daemon-status.js`

- [ ] **Step 1: `src/tools/get-stats.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'get_stats',
  description: 'Counts by kind, oldest/newest timestamps, db size.',
  inputSchema: { type: 'object', properties: {} },
  handler: (_args, { store, dbPath, statSync }) => {
    const s = store.stats();
    const size = statSync(dbPath).size;
    return { ...s, dbPath, sizeBytes: size };
  },
});
```

Note: `statSync` is provided via `ctx` from the MCP entry to keep the tool unit-testable without mocking `fs`.

- [ ] **Step 2: `src/tools/daemon-status.js`**

```js
import { defineTool } from './index.js';

defineTool({
  name: 'daemon_status',
  description: 'Is the clipboard-history-daemon running? When did it start? When was the last capture?',
  inputSchema: { type: 'object', properties: {} },
  handler: (_args, { daemonStatus }) => daemonStatus(),
});
```

- [ ] **Step 3: Commit**

```bash
git add src/tools/get-stats.js src/tools/daemon-status.js
git commit -m "feat(tools): system tools (get_stats, daemon_status)"
```

---

## Task 20: Implement `src/mcp/index.js`

**Files:**
- Create: `src/mcp/index.js`

- [ ] **Step 1: Implement**

```js
#!/usr/bin/env node
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';
import { statSync, existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';
import { listTools, callTool, loadAllTools } from '../tools/index.js';

const DEFAULT_DIR = process.env.CLIPBOARD_DATA_DIR
  || join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const DEFAULT_DB = process.env.CLIPBOARD_DB_PATH || join(DEFAULT_DIR, 'history.db');
const PID_FILE = join(DEFAULT_DIR, 'daemon.pid');

const masterKey = getOrCreateMasterKey();
const store = new Store(DEFAULT_DB, { masterKey });

await loadAllTools();

const ctx = {
  store,
  dbPath: DEFAULT_DB,
  statSync,
  daemonStatus: () => {
    if (!existsSync(PID_FILE)) return { running: false };
    try {
      const pid = parseInt(readFileSync(PID_FILE, 'utf8'), 10);
      process.kill(pid, 0);
      return { running: true, pid };
    } catch {
      return { running: false };
    }
  },
  log: (msg) => process.stderr.write(`[mcp] ${new Date().toISOString()} ${msg}\n`),
};

const server = new Server(
  { name: 'clipboard-history-mcp', version: '0.2.0-alpha.0' },
  { capabilities: { tools: {} } },
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: listTools() }));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  const { name, arguments: args } = req.params;
  try {
    const result = await callTool(name, args, ctx);
    return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
  } catch (err) {
    return {
      isError: true,
      content: [{ type: 'text', text: err instanceof Error ? err.message : String(err) }],
    };
  }
});

const transport = new StdioServerTransport();
await server.connect(transport);

process.on('SIGINT', () => { store.close(); process.exit(0); });
process.on('SIGTERM', () => { store.close(); process.exit(0); });
```

- [ ] **Step 2: Smoke test**

Run: `(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0.1"}}}' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'; sleep 1) | node src/mcp/index.js`

Expected: server returns `tools/list` response with 15 tools.

- [ ] **Step 3: Commit**

```bash
git add src/mcp/index.js
git commit -m "feat(mcp): stdio server with full 15-tool registry wiring"
```

---

## Task 21: Wire MCP smoke test as integration test

**Files:**
- Modify: `tests/integration/daemon-mcp.test.js`

- [ ] **Step 1: Append MCP test**

```js
// at the end of tests/integration/daemon-mcp.test.js
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

describe('mcp: list_tools and list_history through stdio', () => {
  it('handshake + tools/list + tools/call', async () => {
    const mcp = spawn('node', [
      fileURLToPath(new URL('../../src/mcp/index.js', import.meta.url)),
    ], {
      env: { ...process.env, CLIPBOARD_DATA_DIR: tmpDir, CLIPBOARD_DB_PATH: join(tmpDir, 'test.db') },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const responses = [];
    mcp.stdout.on('data', (chunk) => {
      for (const line of chunk.toString('utf8').split('\n').filter(Boolean)) {
        try { responses.push(JSON.parse(line)); } catch { /* */ }
      }
    });
    mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 't', version: '1' } } }) + '\n');
    mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
    mcp.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list' }) + '\n');
    await new Promise((r) => setTimeout(r, 1500));
    mcp.kill();

    expect(responses.find((r) => r.id === 1)).toBeTruthy();
    const list = responses.find((r) => r.id === 2);
    expect(list?.result?.tools?.length).toBeGreaterThanOrEqual(15);
  });
});
```

- [ ] **Step 2: Run it**

Run: `npx vitest run tests/integration/daemon-mcp.test.js`
Expected: 2 integration tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/integration/daemon-mcp.test.js
git commit -m "test(integration): MCP stdio handshake and tools/list"
```

---

## Task 22: Update root MCP registration

**Files:**
- Run user-scope re-registration command

- [ ] **Step 1: Update Claude Code MCP config**

```bash
claude mcp remove clipboard-history
claude mcp add -s user clipboard-history -- node /Users/stock/Documents/wezom/clipboard-history-mcp/src/mcp/index.js
claude mcp list | grep clipboard-history
```

Expected: shows `✓ Connected`.

- [ ] **Step 2: No commit needed (config change is local)**

---

# Milestone 4 — CLI

## Task 23: CLI scaffolding (`src/cli/index.js` + `bin/clipboard-history`)

**Files:**
- Create: `src/cli/index.js`
- Create: `bin/clipboard-history`

- [ ] **Step 1: Implement `src/cli/index.js`**

```js
#!/usr/bin/env node
import { Command } from 'commander';
import { install } from './install.js';
import { uninstall } from './uninstall.js';
import { status } from './status.js';
import { vault } from './vault.js';
import { doctor } from './doctor.js';

const program = new Command();
program.name('clipboard-history').description('macOS clipboard history MCP control plane').version('0.2.0-alpha.0');

program.command('install').option('--window-titles', 'capture window titles too').action(install);
program.command('uninstall').option('--keep-data', 'do not remove DB').action(uninstall);
program.command('status').action(status);
program.command('start').action(() => import('./install.js').then((m) => m.start()));
program.command('stop').action(() => import('./install.js').then((m) => m.stop()));
program.command('vault').argument('<sub>', 'list | unlock <id>').argument('[id]').action(vault);
program.command('doctor').action(doctor);

program.parseAsync(process.argv);
```

- [ ] **Step 2: Create `bin/clipboard-history`**

```js
#!/usr/bin/env node
import('../src/cli/index.js');
```

- [ ] **Step 3: Make executable**

```bash
chmod +x bin/clipboard-history src/cli/index.js
```

- [ ] **Step 4: Commit**

```bash
git add src/cli/index.js bin/clipboard-history
git commit -m "feat(cli): commander scaffold for clipboard-history binary"
```

---

## Task 24: Implement `install` / `uninstall` / `start` / `stop`

**Files:**
- Create: `src/cli/install.js`
- Create: `src/cli/uninstall.js`
- Create: `scripts/launchd.plist.template`

- [ ] **Step 1: Create `scripts/launchd.plist.template`**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>__LABEL__</string>
  <key>ProgramArguments</key>
  <array>
    <string>__NODE__</string>
    <string>__DAEMON_JS__</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>EnvironmentVariables</key>
  <dict>
__ENV_DICT__
  </dict>
  <key>StandardOutPath</key>
  <string>__LOG_PATH__</string>
  <key>StandardErrorPath</key>
  <string>__LOG_PATH__</string>
</dict>
</plist>
```

- [ ] **Step 2: Implement `src/cli/install.js`**

```js
import { spawnSync } from 'node:child_process';
import { writeFileSync, readFileSync, mkdirSync, existsSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '../..');
const LABEL = 'me.kz.clipboard-history';
const PLIST_PATH = join(homedir(), 'Library', 'LaunchAgents', `${LABEL}.plist`);
const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const LOG_PATH = join(DATA_DIR, 'daemon.log');

export function install({ windowTitles = false } = {}) {
  mkdirSync(DATA_DIR, { recursive: true });
  const tpl = readFileSync(join(ROOT, 'scripts/launchd.plist.template'), 'utf8');
  const env = { CLIPBOARD_CAPTURE_WINDOW_TITLE: windowTitles ? '1' : '0' };
  const envDict = Object.entries(env).map(
    ([k, v]) => `    <key>${k}</key>\n    <string>${v}</string>`
  ).join('\n');
  const plist = tpl
    .replace('__LABEL__', LABEL)
    .replace('__NODE__', process.execPath)
    .replace('__DAEMON_JS__', join(ROOT, 'src/daemon/index.js'))
    .replace('__ENV_DICT__', envDict)
    .replace(/__LOG_PATH__/g, LOG_PATH);
  writeFileSync(PLIST_PATH, plist);
  spawnSync('launchctl', ['unload', PLIST_PATH], { stdio: 'ignore' });
  const r = spawnSync('launchctl', ['load', '-w', PLIST_PATH], { stdio: 'inherit' });
  if (r.status !== 0) {
    console.error('launchctl load failed; check permissions.');
    process.exit(1);
  }
  console.log(`Installed → ${PLIST_PATH}`);
  console.log(`Logs    → ${LOG_PATH}`);
}

export function start() {
  spawnSync('launchctl', ['kickstart', '-k', `gui/${process.getuid()}/${LABEL}`], { stdio: 'inherit' });
}

export function stop() {
  spawnSync('launchctl', ['unload', PLIST_PATH], { stdio: 'inherit' });
}
```

- [ ] **Step 3: Implement `src/cli/uninstall.js`**

```js
import { spawnSync } from 'node:child_process';
import { unlinkSync, existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

const LABEL = 'me.kz.clipboard-history';
const PLIST_PATH = join(homedir(), 'Library', 'LaunchAgents', `${LABEL}.plist`);
const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');

export function uninstall({ keepData = false } = {}) {
  if (existsSync(PLIST_PATH)) {
    spawnSync('launchctl', ['unload', PLIST_PATH], { stdio: 'ignore' });
    unlinkSync(PLIST_PATH);
    console.log(`Removed ${PLIST_PATH}`);
  }
  if (!keepData && existsSync(DATA_DIR)) {
    rmSync(DATA_DIR, { recursive: true, force: true });
    console.log(`Removed ${DATA_DIR}`);
  }
}
```

- [ ] **Step 4: Smoke test**

```bash
node bin/clipboard-history install
node bin/clipboard-history status   # task 25 - skip for now
launchctl list | grep clipboard
```

Expected: agent loaded.

- [ ] **Step 5: Commit**

```bash
git add src/cli/install.js src/cli/uninstall.js scripts/launchd.plist.template
git commit -m "feat(cli): install/uninstall/start/stop with launchd plist"
```

---

## Task 25: Implement `status` command

**Files:**
- Create: `src/cli/status.js`

- [ ] **Step 1: Implement**

```js
import { existsSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { Store } from '../core/store.js';

const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');
const PID_FILE = join(DATA_DIR, 'daemon.pid');
const DB = join(DATA_DIR, 'history.db');

export function status() {
  const out = { dataDir: DATA_DIR, dbPath: DB };
  out.dbExists = existsSync(DB);
  if (out.dbExists) out.dbSizeBytes = statSync(DB).size;
  if (existsSync(PID_FILE)) {
    const pid = parseInt(readFileSync(PID_FILE, 'utf8'), 10);
    try { process.kill(pid, 0); out.daemon = { running: true, pid }; }
    catch { out.daemon = { running: false, stalePid: pid }; }
  } else out.daemon = { running: false };
  if (out.dbExists) {
    const store = new Store(DB);
    out.stats = store.stats();
    store.close();
  }
  console.log(JSON.stringify(out, null, 2));
}
```

- [ ] **Step 2: Smoke test**

Run: `node bin/clipboard-history status`
Expected: prints JSON with `daemon.running: true`, `dbExists`, `stats.count`.

- [ ] **Step 3: Commit**

```bash
git add src/cli/status.js
git commit -m "feat(cli): status command"
```

---

## Task 26: Implement `vault` and `doctor` commands

**Files:**
- Create: `src/cli/vault.js`
- Create: `src/cli/doctor.js`

- [ ] **Step 1: Implement `src/cli/vault.js`**

```js
import { join } from 'node:path';
import { homedir } from 'node:os';
import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';

const DB = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp', 'history.db');

export function vault(sub, id) {
  const store = new Store(DB, { masterKey: getOrCreateMasterKey() });
  try {
    if (sub === 'list') {
      const items = store.list({ limit: 500 }).filter((i) => i.primaryKind.startsWith('secret:'));
      console.log(JSON.stringify({ count: items.length, items }, null, 2));
    } else if (sub === 'unlock') {
      if (!id) { console.error('Usage: vault unlock <id>'); process.exit(1); }
      console.log(store.unlockSecret(parseInt(id, 10)));
    } else {
      console.error(`Unknown vault subcommand: ${sub}`);
      process.exit(1);
    }
  } finally {
    store.close();
  }
}
```

- [ ] **Step 2: Implement `src/cli/doctor.js`**

```js
import { spawnSync, execSync } from 'node:child_process';
import { existsSync, accessSync, constants } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { homedir } from 'node:os';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '../..');
const TYPES_BIN = join(ROOT, 'bin/pasteboard-types');
const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp');

export function doctor() {
  const checks = [];

  checks.push(['data dir writable', () => {
    accessSync(DATA_DIR, constants.W_OK | constants.R_OK);
    return DATA_DIR;
  }]);

  checks.push(['Swift helper exists', () => {
    if (!existsSync(TYPES_BIN)) throw new Error(`build with: bash scripts/build-native.sh`);
    return TYPES_BIN;
  }]);

  checks.push(['osascript callable', () => {
    const r = spawnSync('osascript', ['-e', 'return 1'], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error(r.stderr);
    return 'ok';
  }]);

  checks.push(['Keychain accessible', () => {
    const r = spawnSync('security', ['list-keychains'], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error('security CLI failed');
    return 'ok';
  }]);

  checks.push(['accessibility permission for window titles', () => {
    const r = spawnSync('osascript', ['-e', 'tell application "System Events" to get name of front window of (first process whose frontmost is true)'], { encoding: 'utf8', timeout: 1500 });
    if (r.status !== 0 || /not authorized|1002/.test(r.stderr || '')) {
      throw new Error('accessibility permission missing — System Settings → Privacy → Accessibility');
    }
    return 'ok';
  }]);

  for (const [label, fn] of checks) {
    try {
      const out = fn();
      console.log(`✓ ${label}: ${out}`);
    } catch (e) {
      console.log(`✗ ${label}: ${e.message}`);
    }
  }
}
```

- [ ] **Step 3: Smoke test**

Run: `node bin/clipboard-history doctor`
Expected: 5 lines, mostly ✓ (accessibility may be ✗ until granted).

- [ ] **Step 4: Commit**

```bash
git add src/cli/vault.js src/cli/doctor.js
git commit -m "feat(cli): vault list/unlock and doctor diagnostic"
```

---

# Milestone 5 — Polish, tests, release

## Task 27: Migrate-from-v1 helper

**Files:**
- Create: `src/cli/migrate-v1.js`
- Modify: `src/cli/index.js` (add command)

- [ ] **Step 1: Implement migration**

```js
import { join } from 'node:path';
import { homedir } from 'node:os';
import { existsSync, readFileSync } from 'node:fs';
import { Store } from '../core/store.js';
import { getOrCreateMasterKey } from '../core/crypto.js';
import { classify } from '../core/types.js';

const V1_PATH = join(homedir(), '.clipboard-history-mcp', 'history.json');
const V2_DB = join(homedir(), 'Library', 'Application Support', 'clipboard-history-mcp', 'history.db');

export function migrateV1() {
  if (!existsSync(V1_PATH)) {
    console.log('No v1 history.json found — nothing to migrate.');
    return;
  }
  const data = JSON.parse(readFileSync(V1_PATH, 'utf8'));
  const items = Array.isArray(data.items) ? data.items : [];
  const store = new Store(V2_DB, { masterKey: getOrCreateMasterKey() });
  let migrated = 0;
  for (const item of items.reverse()) {
    if (!item?.text) continue;
    const { primaryKind, kinds } = classify(item.text);
    store.addClip({ text: item.text, primaryKind, kinds });
    migrated++;
  }
  store.close();
  console.log(`Migrated ${migrated} clips from v1`);
}
```

- [ ] **Step 2: Wire into CLI**

Edit `src/cli/index.js` — add:

```js
import { migrateV1 } from './migrate-v1.js';
program.command('migrate-v1').action(migrateV1);
```

- [ ] **Step 3: Commit**

```bash
git add src/cli/migrate-v1.js src/cli/index.js
git commit -m "feat(cli): migrate v1 history.json into v2 SQLite"
```

---

## Task 28: E2E test (full cycle: copy → daemon → MCP → unlock)

**Files:**
- Create: `tests/e2e/full-cycle.test.js`

- [ ] **Step 1: Write test**

```js
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Store } from '../../src/core/store.js';
import { startWatcher } from '../../src/daemon/watcher.js';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';

describe('e2e: copy → secret detected → unlock', () => {
  let tmpDir, store, stop, key;
  beforeAll(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'cbhist-e2e-'));
    key = randomBytes(32);
    store = new Store(join(tmpDir, 'test.db'), { masterKey: key });
    stop = startWatcher(store, { intervalMs: 200, log: () => {} });
  });
  afterAll(() => {
    stop();
    store.close();
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('captures a copied OpenAI key as a secret and unlocks it', async () => {
    const fake = 'sk-proj-' + 'A'.repeat(48);
    spawnSync('pbcopy', [], { input: fake });
    await new Promise((r) => setTimeout(r, 600));
    const items = store.list({ limit: 5 });
    const secret = items.find((i) => i.primaryKind?.startsWith('secret:'));
    expect(secret).toBeTruthy();
    expect(secret.text).toBeNull();
    expect(secret.preview).toMatch(/REDACTED/);
    const value = store.unlockSecret(secret.id);
    expect(value).toBe(fake);
  });
});
```

- [ ] **Step 2: Run**

Run: `npx vitest run tests/e2e/full-cycle.test.js`
Expected: 1 test passes.

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/full-cycle.test.js
git commit -m "test(e2e): full secret-capture and unlock cycle"
```

---

## Task 29: GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`
- Create: `.github/dependabot.yml`

- [ ] **Step 1: Write `ci.yml`**

```yaml
name: CI
on:
  push: { branches: [main] }
  pull_request:
jobs:
  test:
    strategy:
      matrix:
        os: [macos-13, macos-14]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '20', cache: 'npm' }
      - run: npm ci
      - run: bash scripts/build-native.sh
      - run: npx vitest run tests/unit
      - run: npx vitest run tests/integration
      - run: npx tsc --noEmit
```

- [ ] **Step 2: Write `dependabot.yml`**

```yaml
version: 2
updates:
  - package-ecosystem: 'npm'
    directory: '/'
    schedule: { interval: 'weekly' }
  - package-ecosystem: 'github-actions'
    directory: '/'
    schedule: { interval: 'weekly' }
```

- [ ] **Step 3: Commit**

```bash
git add .github/
git commit -m "ci: GitHub Actions matrix on macos-13/14 + dependabot"
```

---

## Task 30: README rewrite

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite README**

```markdown
# clipboard-history-mcp

Type-aware, secret-safe macOS clipboard history exposed to Claude via MCP.

```mermaid
flowchart LR
  U[You copy<br/>something] --> D[Daemon<br/>captures + classifies]
  D --> DB[(SQLite + FTS5)]
  C[Claude] -->|MCP stdio| M[15 tools]
  M --> DB
```

## Why

- **Maccy** is great as a clipboard manager but doesn't talk to LLMs.
- The other ~20 `clipboard-mcp` repos on GitHub only read the *current* clipboard — no history.
- `vlad-ds/maccy-clipboard-mcp` reads Maccy's DB but is tied to Maccy.

This project does three things none of the above do **together**:

1. **Type-aware retrieval.** Every clip is classified at capture (`url`, `json`, `code:python`, `sql`, `secret:openai_api_key`, …) and exposed via filtered tools.
2. **Secret-aware capture.** API keys, JWTs, AWS access keys, credit cards (252 patterns from gitleaks) are detected, encrypted at rest (AES-256-GCM, key in macOS Keychain), and never leak to Claude unless you call `unlock_secret(id, reason)`.
3. **Always-on background daemon** under launchd. History is captured even when Claude Code is closed.

## Comparison

|  | Maccy | maccy-clipboard-mcp | clipboard-history-mcp |
|---|---|---|---|
| Captures clipboard | ✓ | (reads Maccy's DB) | ✓ |
| Indexes window titles | ✗ | ✗ | ✓ |
| Type-aware tools (URL/code/SQL/JSON) | ✗ | ✗ | ✓ |
| Secret detection + encryption | ✗ | ✗ | ✓ |
| Standalone (no other app required) | n/a | ✗ | ✓ |
| GUI / hotkey | ✓ | ✗ | ✗ (use Maccy alongside) |

## Install

```bash
git clone https://github.com/d-khomenko/clipboard-history-mcp
cd clipboard-history-mcp
npm install
bash scripts/build-native.sh
node bin/clipboard-history install
claude mcp add -s user clipboard-history -- node "$(pwd)/src/mcp/index.js"
```

Restart Claude Code, then ask: *"List my recent clipboard URLs."*

## Configuration

| Var | Default | Purpose |
|---|---|---|
| `CLIPBOARD_POLL_MS` | `1500` | watcher poll interval |
| `CLIPBOARD_HISTORY_MAX` | `1000` | ring-buffer size |
| `CLIPBOARD_CAPTURE_WINDOW_TITLE` | `0` | opt-in to window title capture (needs Accessibility) |
| `CLIPBOARD_IGNORE_APPS` | (empty) | comma list of app display names to skip |
| `CLIPBOARD_NEVER_STORE_SECRETS` | `0` | paranoid mode — secret metadata only, no ciphertext |

## Tools

`list_history`, `get_item`, `search_history`, `get_urls`, `get_code(language?)`, `get_json`, `get_secrets_index`, `unlock_secret(id, reason)`, `copy_item`, `pin_item`, `tag_item`, `delete_item`, `clear_history`, `get_stats`, `daemon_status`.

See [`docs/superpowers/specs/2026-05-05-clipboard-history-mcp-v2-design.md`](docs/superpowers/specs/2026-05-05-clipboard-history-mcp-v2-design.md) for design details.

## License

MIT.

`vendor/gitleaks.toml` is the gitleaks rule catalog (MIT) — see `vendor/README.md`.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: rewrite README for v2 architecture"
```

---

## Task 31: Add LICENSE, CHANGELOG, CONTRIBUTING

**Files:**
- Create: `LICENSE`
- Create: `CHANGELOG.md`
- Create: `CONTRIBUTING.md`

- [ ] **Step 1: `LICENSE`**

```
MIT License

Copyright (c) 2026 Dmytro Khomenko

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 2: `CHANGELOG.md`**

```markdown
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] — 2026-05-DD

### Added
- launchd-managed background daemon
- SQLite + FTS5 storage
- Type-aware classification (URL, JSON, SQL, shell, code:lang via flourite)
- Secret detection via vendored gitleaks rules + JWT + Luhn-validated credit cards
- AES-256-GCM secret encryption with macOS Keychain–backed master key
- Window-title capture (opt-in, Accessibility permission)
- 15-tool MCP surface (read / write / system)
- CLI: `install | uninstall | status | start | stop | vault | doctor | migrate-v1`

### Removed
- v1 single-process JSON-store implementation

## [0.1.0] — 2026-05-05

### Added
- Initial single-process MCP server polling pbpaste
```

- [ ] **Step 3: `CONTRIBUTING.md`**

```markdown
# Contributing

Thanks for considering a contribution!

## Dev setup

```bash
git clone https://github.com/d-khomenko/clipboard-history-mcp
cd clipboard-history-mcp
npm install
bash scripts/build-native.sh
npm test
```

## Tests

- `npm test` — unit + integration
- `npm run test:e2e` — copies real things to your clipboard, run separately

## Style

- ESM only.
- No transpilation; we run JS through Node directly.
- Comments only when the *why* is non-obvious.
- Prefer small focused files; the file map in `docs/superpowers/plans/` is canonical.

## Commits

Conventional commits preferred (`feat:`, `fix:`, `docs:`, `chore:`, `test:`).
```

- [ ] **Step 4: Commit**

```bash
git add LICENSE CHANGELOG.md CONTRIBUTING.md
git commit -m "docs: add LICENSE (MIT), CHANGELOG, CONTRIBUTING"
```

---

## Task 32: Update `.gitignore` for v2

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Replace contents**

```
node_modules/
coverage/
dist/
*.log
.DS_Store
.env
.env.*
*.db
*.db-shm
*.db-wal
/bin/pasteboard-types
.vitest-cache/
```

- [ ] **Step 2: Commit**

```bash
git add .gitignore
git commit -m "chore: gitignore v2 build artefacts and DB files"
```

---

## Task 33: Self-review pass on v1 leftover artefacts

**Files:**
- Audit: any references to `~/.clipboard-history-mcp/history.json` outside migration code

- [ ] **Step 1: Search for v1 paths**

Run: `grep -rn '\.clipboard-history-mcp/history.json' src tests`
Expected: only matches inside `src/cli/migrate-v1.js`.

- [ ] **Step 2: If extra matches found, fix and commit**

(Likely empty — v1 was deleted in Task 2.)

- [ ] **Step 3: Run full test suite**

```bash
npm test
npm run typecheck
```

Expected: all pass.

- [ ] **Step 4: Commit any fixes**

```bash
git add -u
git commit -m "chore: clean up residual v1 references" || true
```

---

## Task 34: Tag and push v0.2.0

**Files:**
- (none — git tag)

- [ ] **Step 1: Verify clean working tree**

Run: `git status`
Expected: clean.

- [ ] **Step 2: Confirm `version` in `package.json`**

Run: `grep '"version"' package.json | head -1`
Expected: `"version": "0.2.0-alpha.0"` (bump to `0.2.0` here if shipping stable).

- [ ] **Step 3: Tag**

```bash
git tag -a v0.2.0 -m "v0.2.0 — type-aware, secret-safe clipboard history MCP"
git push origin main --follow-tags
```

- [ ] **Step 4: Verify on GitHub**

Run: `gh release create v0.2.0 --generate-notes` (or skip and create manually).
Expected: release page lists tag.

---

## Self-review

I scanned the spec section-by-section against this plan. Coverage:

- §1 Why → README (Task 30) ✓
- §2 Goals/Non-goals → covered by Phase 1 task scope ✓
- §3 Architecture → Tasks 7, 13, 20 ✓
- §4 Data model → Task 7 (full schema) ✓
- §5 Capture flow → Tasks 9–14 ✓
- §6 Type detection → Task 4 ✓
- §7 Privacy → Tasks 3, 8 (encryption), 14, 17, 28 (e2e) ✓
- §8 MCP tools (15) → Tasks 16, 17, 18, 19 (15 tools) ✓
- §9 CLI → Tasks 23–26 ✓
- §10 Configuration → Task 13 (env vars), Task 24 (plist EnvironmentVariables) ✓
- §11 Project layout → matches file map at top of this plan ✓
- §12 Testing → unit (3, 4, 6, 7, 8), integration (14, 21), e2e (28) ✓
- §13 Open-source polish → Tasks 29, 30, 31, 32 ✓
- §14 Risks → mitigated:
  - #1 (gitleaks refresh) — Task 5 + dependabot
  - #2 (Accessibility) — Task 26 doctor
  - #3 (Keychain ACL) — Task 26 doctor
  - #4 (transient types) — Task 9 (Swift helper) + Task 10 (Node integration)
  - #5 (v1 migration) — Task 27
  - #6 (DB growth) — covered by `CLIPBOARD_HISTORY_MAX` (still pending — see §15 below)
  - #7 (multi-Mac) — phase 2

### Gaps found and patched inline

- ~~No retention enforcement (HISTORY_MAX)~~ → would have been spec gap #6 risk. **Adding Task 13.5 below as inline patch.**
- ~~`untag_item`~~ rolled into `tag_item` with `remove: true`. Reflected in spec section 8 implicitly; documented in Task 18 step 3.

### Inline patch — Task 13.5: retention enforcement

After Task 13, add a 2-step task:

**Files:**
- Modify: `src/daemon/watcher.js`

- [ ] **Step 1: After each insert, prune oldest beyond MAX**

In `src/daemon/watcher.js`, after the `store.addClip` and `store.addSecret` calls, call:

```js
store.pruneOldest({ keep: opts.maxItems ?? 1000 });
```

Add to `Store` (`src/core/store.js`):

```js
pruneOldest({ keep }) {
  return this.db.prepare(
    `DELETE FROM clips WHERE id IN (SELECT id FROM clips ORDER BY is_pinned DESC, last_copied_at DESC LIMIT -1 OFFSET ?)`
  ).run(keep).changes;
}
```

- [ ] **Step 2: Commit**

```bash
git add src/daemon/watcher.js src/core/store.js
git commit -m "feat(daemon): enforce CLIPBOARD_HISTORY_MAX retention"
```

---

# Execution

**Plan complete and saved to `docs/superpowers/plans/2026-05-05-clipboard-history-mcp-v2-implementation.md`.**

Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
