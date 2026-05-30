# litecast

A super-lightweight, native Spotlight/Raycast-style launcher for macOS, written in Rust.

litecast runs as a background menu-bar / accessory app (no Dock icon) and pops up a
borderless search panel on a global hotkey. It is built directly on AppKit via
[`objc2`](https://crates.io/crates/objc2) (no web view, no cross-platform UI toolkit),
so it stays fast and lean.

## Goals

- Concise, performant, minimal CPU overhead.
- Native AppKit UI (an `NSPanel`), idle when hidden.
- Prefer hand-rolled implementations for small features; only depend on a crate when it
  is genuinely leaner or faster than what we'd write.

## Features

- Global hotkey to toggle the search panel.
- Fuzzy app launcher.
- File search (backed by the macOS Spotlight index via `mdfind`).
- Inline calculator (hand-rolled evaluator).
- Web-search fallback (opens the default browser).
- Clipboard history (the `clip` keyword).
- User-defined custom commands + an external [plugin protocol](docs/plugins.md).
- AI query (Claude / OpenAI / Cursor-compatible), with API keys stored in the macOS Keychain.
- Screenshot capture sent to an AI vision model.
- Small, opt-out UI delights (playful placeholders, fade-in, easter eggs, wandering critters).

## Hotkeys

- `Option + Space` - toggle the launcher panel.
- `Option + Shift + Space` - capture a screen region and ask the AI about it.

## Usage

Open the panel and start typing:

- Type an app or file name to launch/open it.
- Type a math expression (e.g. `12 * (3 + 4)`) for an instant result.
- Type `clip` to browse clipboard history (`clip foo` to filter).
- Type `? your question` to ask the configured AI backend.
- Type `setkey <api-key>` to store the API key for the active AI backend in the Keychain.
- Anything else offers a web search.

Arrow keys move the selection, `Enter` activates, `Esc` dismisses.

## Configuration

A commented config file is created on first run at:

```
~/Library/Application Support/litecast/config.toml
```

It controls the web-search URL, custom commands, AI backend (provider/model/endpoint),
and UI toggles. Plugins go in `.../litecast/plugins/` (see [docs/plugins.md](docs/plugins.md)),
and wandering-critter GIFs go in `.../litecast/critters/`.

## Permissions

- Global hotkeys use Carbon and need **no** Accessibility permission.
- The screenshot feature uses the built-in `screencapture`, which requires the
  **Screen Recording** permission (macOS prompts on first use).
- The AI feature needs outbound network access.

## Build & run

```bash
# Dev build and run
cargo run

# Release .app bundle (no Dock icon; runs as a menu-bar/accessory agent)
./scripts/bundle.sh
open target/litecast.app
```

Requires a recent stable Rust toolchain (1.85+) and macOS 11+.

Because the app is not codesigned/notarized yet, Gatekeeper will block the first
launch. Right-click `litecast.app` and choose **Open**, then confirm. (Signing
and notarization are planned for a later milestone.)

## License

MIT
