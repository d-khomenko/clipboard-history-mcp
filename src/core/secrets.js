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
    // Skip path-only rules (no regex field)
    if (!r.regex) return null;
    try {
      const allowPatterns = (r.allowlists || []).flatMap((al) =>
        (al.regexes || []).map((rx) => new RegExp(rx))
      );
      return {
        id: r.id,
        kind: r.id,
        re: new RegExp(r.regex, 'g'),
        keywords: r.keywords || [],
        allowPatterns,
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
    allowPatterns: [],
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
      // Check allowlists — if any allowlist pattern matches, skip this hit
      const allowed = rule.allowPatterns.some((ap) => ap.test(value));
      if (allowed) continue;
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
