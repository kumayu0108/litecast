# litecast v0.1.0 — first release

**litecast** is a super-lightweight, native keyboard launcher for macOS, written in Rust. It is built directly on AppKit via [`objc2`](https://crates.io/crates/objc2) — no web view, no Electron, no cross-platform UI toolkit — so it stays fast and lean, and idles when hidden. Press a global hotkey, a borderless search panel appears, you type, and you act.

This is the first public release.

## Highlights

**Launcher & search**
- Global hotkey (**Option + Space** by default) to toggle a borderless `NSPanel` search panel.
- Fuzzy app launcher and file search (backed by the macOS file index via `mdfind`).
- Frecency ranking — frequently and recently used items drift to the top.
- Recents on open — an empty query shows this session's recent activity and last AI answer (in-memory, never persisted).
- Category filters (`All · Apps · Files · Clipboard · Calc · Web · Commands · Emoji · AI`): click a chip, type an `@` prefix (e.g. `@apps safari`), or cycle with `Tab`. `Cmd + 1…9` selects the Nth chip, Spotlight-style.

**Built-in tools**
- Inline calculator (hand-rolled evaluator).
- Unit & currency conversion (`10 km in mi`, `100 usd to eur`).
- Developer tools — base64, URL encode/decode, MD5/SHA-1/SHA-256, UUID v4, random passwords, lorem ipsum, JSON pretty/minify (all hand-rolled, no network).
- Color, number-base & timestamp converters (`#ff8800`, `255 to hex`, `epoch 1700000000`).
- Date & time — world clock (`time in Tokyo`), date math (`days until 25 Dec`), and non-blocking timers (`timer 5m`).
- Emoji & symbol picker (`emoji` keyword or a `:` prefix).
- Offline dictionary & spell (`define <word>`, `spell <word>`).

**System & productivity**
- System commands — lock, sleep, dark mode, volume, Wi-Fi, Bluetooth, brightness, caffeinate, eject, Focus, empty trash, restart, and more.
- File power actions (`reveal`, `ql`, `copypath`, `folder`) plus `recent` and `downloads` listings.
- Calendar & reminders (`today`/`agenda`, `remind …`, `event …`) via AppleScript.
- Network info (`ip`, `myip`, `ports`, `port <n>`, `wifi networks`).
- Quick notes (`note <text>`) to a plain-text file, optionally mirrored to Apple Notes.
- Media controls (`play`, `pause`, `next`, `prev`, `now playing`) for Spotify/Music.
- Clipboard history with pins and text/link/image types (`clip`).
- Text snippets (`snip`), and quicklinks — parameterized `{query}` URLs (e.g. `ghr rust-lang/rust`).
- Browser bookmark/history search (`bm` / `hist`) for Chrome, Brave, Edge, Chromium, Vivaldi.
- Process manager (`kill` / `ps`) — confirm-then-SIGTERM your own processes.
- Window management (opt-in; needs Accessibility) — snap/resize the frontmost window with `win`.
- User-defined custom commands (with aliases), app commands (`@keyword` actions), and an external [plugin protocol](plugins.md).
- Custom global hotkeys — bind any combo to open a URL, run a shell command, or fire a named command.

**AI**
- Ask the AI from the launcher with `? your question`, then keep typing to continue a multi-turn chat.
- Providers: **Anthropic Claude**, **OpenAI**, **Google Gemini**, any **OpenAI-compatible** endpoint, and **local Ollama** (no API key needed for local).
- API keys are stored in the macOS **Keychain** (service `litecast`), never in a config file.
- Quick AI commands — `translate`/`tr`, `summarize`/`sum`, `fixgrammar`/`fix`, `improve`.
- Screenshot-to-AI — **Option + Shift + Space** captures a screen region and asks an AI vision model about it.
- Guided in-app key setup (`setup`) and `setkey <api-key>`.

**App shell & settings**
- Native macOS app shell with a Dock icon and a menu-bar extra.
- Native, **resizable** Settings/Preferences window with a sidebar listing every config section (General, Hotkeys, AI, Commands, App commands, Quicklinks, Snippets, Clipboard, Conversion, Window, Menu, Notes, Date & time, Scripts, Git, New file, Pomodoro, Color), in-app help under every setting, add/remove list editors, a **hotkey recorder**, and an opt-in **Launch at login**.
- Configured via `config.toml`; saving from Preferences applies immediately (the query engine is rebuilt and hotkeys re-registered without restarting), and **Reload from disk** picks up external edits.

## Getting started

```bash
# Build & assemble the .app bundle (use env -u to avoid a stale CARGO_TARGET_DIR)
env -u CARGO_TARGET_DIR ./scripts/bundle.sh
open target/litecast.app
```

1. The app is ad-hoc signed and not notarized, so Gatekeeper blocks the first launch — **right-click `litecast.app` → Open**, then confirm. (You only do this once.)
2. Press **Option + Space** to toggle the launcher panel.
3. Open **Settings** from the menu-bar **⌘** icon → **Settings…**, or the **litecast** app menu → **Settings…** (`⌘,`).
4. For AI features, open the panel and type **`setup`**, then **`setkey <your-api-key>`** — the key is stored in the macOS **Keychain**. For local AI, set `provider = "ollama"` in `[ai]` and run a local model (`ollama pull llama3.2`) — no key required.
5. Optional: in **Settings → General**, turn on **Launch litecast at login**.

## Requirements

- **macOS 11+**.
- A recent stable **Rust toolchain (1.85+)** to build from source.
- Optional: a running **[Ollama](https://ollama.com)** instance for local AI (no API key); otherwise an API key for Anthropic / OpenAI / Gemini / an OpenAI-compatible endpoint.

## Known limitations / notes

- The app is **ad-hoc signed and not notarized**, so Gatekeeper prompts on first open (right-click → Open). It is **not on the App Store**.
- File search relies on the **macOS Spotlight index** (`mdfind`); results are only as complete as that index.
- **Cmd + Space** conflicts with Spotlight by default. To use it, free it in System Settings → Keyboard → Keyboard Shortcuts → Spotlight, then set `toggle = "Cmd+Space"`. Option + Space works out of the box.
- The screenshot feature needs **Screen Recording** permission; **window management** is the only feature needing **Accessibility** and is off by default.
- `cargo run` (dev) re-signs each build, so macOS re-prompts for Keychain access every launch — use the bundled app for a prompt-free experience.

## License

MIT. See [LICENSE](../LICENSE).
