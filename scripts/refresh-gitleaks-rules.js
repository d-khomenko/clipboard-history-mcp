import { writeFileSync } from 'node:fs';

const RULES_URL = 'https://raw.githubusercontent.com/gitleaks/gitleaks/master/config/gitleaks.toml';

const res = await fetch(RULES_URL);
if (!res.ok) {
  console.error(`Fetch failed: ${res.status}`);
  process.exit(1);
}
const body = await res.text();
writeFileSync(new URL('../vendor/gitleaks.toml', import.meta.url), body);
console.log(`Wrote ${body.length} bytes to vendor/gitleaks.toml`);
