# litecast security

litecast is a **non-sandboxed** macOS launcher with intentional power-user
capabilities: shell commands, AppleScript automation, plugins, and network
access. This document describes trust boundaries, what the app can do, and the
defense-in-depth measures built in.

## Trust boundaries

Treat these as trusted input — anyone who can edit them controls what litecast
can run on your behalf:

| Location | Capability |
| --- | --- |
| `~/Library/Application Support/litecast/config.toml` | Custom commands, hotkeys, AI endpoints, app `@`-commands |
| `~/Library/Application Support/litecast/plugins/` | Executable plugins that return launcher actions |
| `~/Library/Application Support/litecast/scripts/` | User script commands |

If you did not author these files, do not run litecast with them unchanged.

## What litecast can do

- **Shell**: config `kind = "shell"`, hotkeys, and plugin `shell` actions run
  commands via `/bin/sh -c` (plugin shell actions require confirmation).
- **Automation**: `@`-commands and notes can drive Terminal, Finder, and Apple
  Notes via AppleScript (user arguments are passed as argv, not interpolated
  into script source).
- **Filesystem**: file search, quick file creation, git helpers, and clipboard
  image storage under the support directory.
- **Network**: AI queries (HTTPS to configured endpoints), currency rates, and
  opening URLs in the browser.
- **Accessibility** (opt-in): window management and menu-bar search when
  enabled in Settings.

## Protections

### Path traversal

`new file` / `new folder` and templates validate paths with `safe_join` so
names like `../../.ssh/id_rsa` cannot escape the configured base directory.
`CreatePath` rejects paths containing `..` as a second line of defense.

### AppleScript injection

`kind = "applescript"` app commands pass user input as `osascript` argv
(`item 1 of argv`), not as substituted script source. Templates that still
contain `{query}` log a one-time stderr warning.

### AI endpoint SSRF

Cloud provider endpoints must use HTTPS and known host suffixes; private and
link-local addresses are blocked. Ollama is restricted to localhost. Set
`allow_private_endpoint = true` under `[ai]` only if you intentionally point
openai-compatible providers at a private host.

### API key display

`setkey` shows a masked preview (`••••••••abcd`) in the search UI; the full key
is stored only in the Keychain on Enter.

### Plugin shell confirmation

Plugin actions with `"action": "shell"` are wrapped in a two-step confirm flow
(same pattern as process kill).

### URL scheme filtering

`@safari` and `Action::Open` block dangerous schemes (`javascript:`, `data:`,
etc.); only `http://` and `https://` URL opens are allowed.

### Clipboard secrets

When `[clipboard] skip_secrets = true` (default), clipboard history skips text
that looks like API keys, passwords, tokens, or high-entropy secrets. Skipped
content is never logged.

## Recommendations

- **Code signing**: build with `./scripts/bundle.sh` and prefer a stable signing
  identity so macOS can verify the app has not been tampered with.
- **File permissions**: restrict write access to
  `~/Library/Application Support/litecast/` to your user account.
- **Review config and plugins** before enabling on a shared or untrusted machine.

## Out of scope (future work)

- Full App Sandbox migration (would require re-architecting subprocess, network,
  and filesystem access).
- Encrypting `clipboard.json` at rest (heuristics + opt-out is the v1 approach).

See also: [features](features.md), [plugins](plugins.md).
