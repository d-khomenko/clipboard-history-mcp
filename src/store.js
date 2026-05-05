import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { homedir } from "node:os";
import { randomUUID } from "node:crypto";

const DEFAULT_DIR = join(homedir(), ".clipboard-history-mcp");
const DEFAULT_FILE = join(DEFAULT_DIR, "history.json");
const MAX_ITEMS = Number(process.env.CLIPBOARD_HISTORY_MAX) || 500;
const MAX_TEXT_BYTES = Number(process.env.CLIPBOARD_MAX_TEXT_BYTES) || 1_000_000;

function ensureFile(path) {
  if (!existsSync(dirname(path))) mkdirSync(dirname(path), { recursive: true });
  if (!existsSync(path)) writeFileSync(path, JSON.stringify({ items: [] }, null, 2));
}

export class HistoryStore {
  constructor(path = DEFAULT_FILE) {
    this.path = path;
    ensureFile(this.path);
    this.cache = this._read();
  }

  _read() {
    try {
      const raw = readFileSync(this.path, "utf8");
      const parsed = JSON.parse(raw);
      if (!parsed || !Array.isArray(parsed.items)) return { items: [] };
      return parsed;
    } catch {
      return { items: [] };
    }
  }

  _write() {
    writeFileSync(this.path, JSON.stringify(this.cache, null, 2));
  }

  add(text) {
    if (typeof text !== "string" || text.length === 0) return null;
    if (Buffer.byteLength(text, "utf8") > MAX_TEXT_BYTES) return null;
    const last = this.cache.items[0];
    if (last && last.text === text) return last;
    const item = {
      id: randomUUID(),
      text,
      length: text.length,
      preview: text.slice(0, 200),
      createdAt: new Date().toISOString(),
    };
    this.cache.items.unshift(item);
    if (this.cache.items.length > MAX_ITEMS) {
      this.cache.items.length = MAX_ITEMS;
    }
    this._write();
    return item;
  }

  list({ limit = 20, offset = 0 } = {}) {
    return this.cache.items.slice(offset, offset + limit).map(stripText);
  }

  get(id) {
    return this.cache.items.find((i) => i.id === id) || null;
  }

  search({ query, limit = 20 }) {
    if (!query) return [];
    const needle = query.toLowerCase();
    return this.cache.items
      .filter((i) => i.text.toLowerCase().includes(needle))
      .slice(0, limit)
      .map(stripText);
  }

  clear() {
    const count = this.cache.items.length;
    this.cache.items = [];
    this._write();
    return count;
  }

  stats() {
    const items = this.cache.items;
    return {
      count: items.length,
      max: MAX_ITEMS,
      oldest: items.at(-1)?.createdAt ?? null,
      newest: items[0]?.createdAt ?? null,
      file: this.path,
    };
  }
}

function stripText(item) {
  const { text, ...rest } = item;
  return rest;
}
