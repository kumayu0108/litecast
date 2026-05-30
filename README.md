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

## Planned features (v1)

- Global hotkey to toggle the search panel.
- Fuzzy app launcher.
- File search (backed by the macOS Spotlight index via `mdfind`).
- Inline calculator.
- Web-search fallback (opens the default browser).
- Clipboard history.
- User-defined custom commands + an external plugin protocol.
- AI query (Claude / OpenAI / Cursor-compatible), with API keys stored in the macOS Keychain.
- Screenshot capture sent to an AI vision model.
- Small, opt-out UI delights.

## Status

Early development. See the build instructions below.

## Build

```bash
cargo build --release
```

Requires a recent stable Rust toolchain and macOS.

## License

MIT
