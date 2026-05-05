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
