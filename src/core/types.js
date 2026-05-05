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
