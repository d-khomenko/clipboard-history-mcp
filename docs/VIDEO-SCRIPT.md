# Video script — clipboard-history-mcp launch demo

A 60–75 second storyboard for AI video generators (Sora, Veo, Runway, Pika, Synthesia, etc.).

**Vibe:** clean, modern, dark-mode developer aesthetic. Think Linear / Raycast launch videos. No real screen recording — every scene is generated.

**Voice:** Calm Ukrainian male, ~25–30, slightly low energy, dry humor. (English voice version included at the bottom.)

**Music:** Minimal lo-fi or ambient synth, `~75–85 BPM`. Drops out during voiceover beats, returns underneath.

**Aspect:** 16:9 for landing page, 9:16 vertical cut for socials.

---

## Scene-by-scene

### 1. Hook — "the clipboard amnesia problem" (0:00–0:08)

**Visual:** Macro shot of fingers on a MacBook keyboard, hammering ⌘C ⌘V ⌘C ⌘V in rapid succession. Translucent ghost-snippets fly off the screen and dissolve into the background — URLs, JSON, an API key, a SQL query. Each ghost fades faster than the last. Camera slowly pulls back.

**On-screen text** (typewriter, top-left): `47 clips today.`

**Voiceover (UK):**
> "Скопіював ключ з OpenAI вранці, ввечері — де він був? Maccy пам'ятає. Claude — ні."

**Voiceover (EN):**
> "You copied an API key this morning. By evening — where? Maccy remembers. Claude doesn't."

---

### 2. The disconnect (0:08–0:18)

**Visual:** Split screen. Left half: stylized Maccy clipboard manager with a list of clips scrolling. Right half: a Claude chat window. A user types: *"What URLs did I copy from GitHub today?"* Claude replies with an apologetic shrug emoji. Animated dotted line tries to cross from Maccy → Claude and **breaks** in the middle with a small `404` icon.

**On-screen text** (bottom): `No bridge between your clipboard and your LLM.`

**Voiceover (UK):**
> "На GitHub двадцять MCP-серверів для буферу. Жоден не пам'ятає історію. Maccy не вміє говорити з LLM. Тут — рішення."

**Voiceover (EN):**
> "Twenty clipboard MCP servers on GitHub. None remember history. Maccy doesn't talk to LLMs. Here's the fix."

---

### 3. The fix — drop the .mcpb (0:18–0:28)

**Visual:** Cinematic close-up of a `.mcpb` file icon (looks like a metallic capsule with the Anthropic / Claude orange accent) being dragged into Claude Desktop's Extensions panel. The capsule shimmers, dissolves into UI, and a green "✓ Installed" toast slides in. Cut to a tiny `clipboard-history-mcp` daemon icon flickering to life in the menu bar (rust-colored gear).

**On-screen text** (centered, fades in): `clipboard-history-mcp v0.3` → `Single Rust binary · 6.2 MB · zero deps`

**Voiceover (UK):**
> "Один Rust-бінарник. Один drag-and-drop. Без Node, без Python, без додаткових застосунків."

**Voiceover (EN):**
> "One Rust binary. One drag. No Node, no Python, no companion apps."

---

### 4. Demo — type-aware retrieval (0:28–0:40)

**Visual:** Stylized terminal/Claude chat overlay. User prompt appears character-by-character:

> *"Покажи мені всі URL з OpenAI які я копіював сьогодні."*

Claude responds with an animated table:

| `url` | source app | when |
|---|---|---|
| `platform.openai.com/api-keys` | Safari | 14:23 |
| `openai.com/research` | Arc | 09:11 |
| `github.com/openai/openai-python` | Cursor | 11:47 |

Rows slide in from the right with a subtle stagger.

**On-screen badge** (top-right): `tool: get_urls`

**Voiceover (UK):**
> "Кожен кліп класифікований при захопленні: URL, JSON, код Python, SQL, shell. Запит — і Claude дає тобі точно те що треба."

**Voiceover (EN):**
> "Every clip classified at capture: URL, JSON, Python code, SQL, shell. Ask — get exactly that."

---

### 5. Demo — secret detection + Touch ID (0:40–0:55)

**Visual:** User prompt:

> *"Знайди той API ключ з OpenAI dashboard."*

Claude replies with a redacted card:

```
🔒  openai_api_key
    sourceApp: Safari
    windowTitle: "API Keys — OpenAI Platform"
    lastChars: …ab12
    [requires unlock]
```

User prompt:

> *"Розблокуй його — потрібен для скрипта."*

A native macOS Touch ID dialog slides in: "Reveal stored secret #23: needed for script". A finger silhouette taps. The dialog fades. Claude responds: `sk-proj-***...ab12 — restored to clipboard.` ⌘V is implied.

**On-screen text** (bottom): `AES-256-GCM · macOS Keychain · biometric ACL`

**Voiceover (UK):**
> "Секрети детектяться 252-ма gitleaks-патернами і шифруються відразу. Claude бачить тільки метадату. Розблокувати — тільки через Touch ID, з обов'язковою причиною в логах."

**Voiceover (EN):**
> "Secrets caught by 252 gitleaks patterns, encrypted at capture. Claude sees only metadata. Unlocking needs Touch ID — with a mandatory audit reason."

---

### 6. The architecture — quick reveal (0:55–1:05)

**Visual:** Smooth zoom-out. The chat window shrinks into a node labeled `Claude`. Around it, an animated mermaid-style graph builds:

```
[ NSPasteboard ] → [ Rust daemon (launchd) ] → [ SQLite + FTS5 ]
                                                    ↑↓
[ Claude ] ←─ stdio MCP ─→ [ 15 tools ] ─→ [ AES-256-GCM Vault ]
                                              ↑
                                        [ Touch ID ]
```

Each node lights up as the connection draws. Subtle particle effects on data-flow lines.

**Voiceover (UK):**
> "Бекграунд-демон під launchd. SQLite з FTS5 для пошуку. Чотириста рядків Rust. Все локально."

**Voiceover (EN):**
> "Background daemon under launchd. SQLite with FTS5. ~400 lines of Rust. Everything local."

---

### 7. Outro — call to action (1:05–1:15)

**Visual:** Black background. White typewriter text:

```
brew install d-khomenko/tap/clipboard-history-mcp
```
*(or)*
```
github.com/d-khomenko/clipboard-history-mcp
```

A small terminal cursor blinks. The Rust ferris crab icon ◆ sits beside the URL. End frame.

**Voiceover (UK):**
> "MIT-ліцензія. Open-source. Лінк у описі."

**Voiceover (EN):**
> "MIT-licensed. Open-source. Link in description."

---

## Style notes for the generator

- **Color palette:** charcoal background `#0d0d0f`, accents in Anthropic orange `#cc7733` and Rust ferris orange `#ce422b`. Avoid pure white; use `#e8e6e1` for text.
- **Typography:** Inter or SF Pro for UI. JetBrains Mono for code. Geist Mono works too.
- **Motion:** prefer subtle spring-eased transitions, no aggressive zooms. Particle effects only on data-flow lines.
- **Mac UI elements** must look modern (macOS Sequoia / 26 era — rounded corners, glass blur, dark mode default).
- **No real personal data anywhere.** All clipboard contents in demos are synthetic. Never show a real-looking API key in plaintext (the redacted form `…ab12` is the only acceptable rendering).
- **Voice timing:** target 80–90% of scene duration for VO; leave 10–20% breathing room at the end of each scene.

## Suggested generators per scene

| Scene | Best tool | Why |
|---|---|---|
| 1 (keyboard macro) | Sora / Veo | photorealistic hands |
| 2 (split-screen UI) | Runway gen-3 + Premiere comp | UI compositing |
| 3 (`.mcpb` drag) | Pika / Genmo + manual UI overlay | object animation |
| 4 (chat with table) | After Effects template + ElevenLabs VO | clean motion graphics |
| 5 (Touch ID modal) | Custom macOS UI mockup → animate in AE | needs accuracy |
| 6 (architecture graph) | Manim / Animated SVG / motion-canvas | technical diagram |
| 7 (terminal outro) | After Effects + Geist Mono | trivial |

If you want a single end-to-end generator: **Synthesia** for VO + Runway for visuals, then comp in DaVinci Resolve. Realistically, a 60-second cut takes 4–6 hours from this script with one editor.

## VO script — pure text (for ElevenLabs / Murf.ai etc.)

### Ukrainian (run all scenes, ~50 seconds at conversational pace)

```
Скопіював ключ з OpenAI вранці, ввечері — де він був? Maccy пам'ятає. Claude — ні. На GitHub двадцять MCP-серверів для буферу. Жоден не пам'ятає історію. Maccy не вміє говорити з LLM. Тут — рішення. Один Rust-бінарник. Один drag-and-drop. Без Node, без Python, без додаткових застосунків. Кожен кліп класифікований при захопленні: URL, JSON, код Python, SQL, shell. Запит — і Claude дає тобі точно те що треба. Секрети детектяться 252-ма gitleaks-патернами і шифруються відразу. Claude бачить тільки метадату. Розблокувати — тільки через Touch ID, з обов'язковою причиною в логах. Бекграунд-демон під launchd. SQLite з FTS5. Чотириста рядків Rust. Все локально. MIT-ліцензія. Open-source. Лінк у описі.
```

### English

```
You copied an API key this morning. By evening — where? Maccy remembers. Claude doesn't. Twenty clipboard MCP servers on GitHub. None remember history. Maccy doesn't talk to LLMs. Here's the fix. One Rust binary. One drag. No Node, no Python, no companion apps. Every clip classified at capture: URL, JSON, Python code, SQL, shell. Ask — get exactly that. Secrets caught by 252 gitleaks patterns, encrypted at capture. Claude sees only metadata. Unlocking needs Touch ID — with a mandatory audit reason. Background daemon under launchd. SQLite with FTS5. About 400 lines of Rust. Everything local. MIT-licensed. Open-source. Link in description.
```

---

## Shorter cut (30 sec, vertical / TikTok)

Use scenes 1, 4, 5, 7 only. Cut to:

1. **0:00–0:06** keyboard hook + "47 clips today" (Scene 1)
2. **0:06–0:14** type-aware demo (Scene 4 condensed)
3. **0:14–0:24** Touch ID secret unlock (Scene 5 condensed)
4. **0:24–0:30** terminal install line + GitHub URL (Scene 7)

VO: "47 clips today. Claude doesn't remember any. Until now. One Rust binary. Type-classified, secret-encrypted, Touch ID-gated. MIT, open-source. Link below."
