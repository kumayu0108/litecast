# litecast features & keywords

Quick reference for the built-in providers, their triggers, and the related
`config.toml` sections. All config fields use `serde(default)`, so adding any of
these to an existing config is backward compatible.

## Search filters (scope results to a category)

Scope results to a single category two interchangeable ways:

- **Typed prefix:** start the query with an `@token` followed by a space:
  `@apps safari`, `@files report`, `@calc 10 km in mi`, `@web rust`,
  `@cmd lock`, `@emoji fire`, `@ai explain x`. `@clip` (and any token alone)
  scopes with an empty query.
- **Tab chip:** press **Tab** to cycle the filter forward
  (`All -> Apps -> Files -> Clipboard -> Calc -> Web -> Commands -> Emoji -> AI -> All`)
  and **Shift+Tab** to cycle backward. A chip in the search area always shows the
  state: a faint "⇥ Filter" hint when unfiltered, or the active category as an
  accent pill.

Both drive the same active filter. **Esc** exits AI chat first (if active), then
clears an active filter, then closes the panel. Tokens and the categories they
map to:

| Token | Category | Includes (source labels) |
| --- | --- | --- |
| `@apps` | Apps | App |
| `@files` | Files | File |
| `@clip` | Clipboard | Clip |
| `@calc` | Calc / Conversions | Calc, Convert |
| `@web` | Web | Web |
| `@cmd` | Commands | Command, Quicklink, Snippet, System, Plugin |
| `@emoji` | Emoji | Emoji |
| `@ai` | AI | AI |

When a filter is active, only that category's providers run, so unrelated (and
sometimes expensive, like the `mdfind` file search) work is skipped.

## Ranking: frecency

Every activation is recorded to `usage.json` in the support dir. Frequently and
recently used items (apps, files, commands, quicklinks, snippets, system
commands) receive a bounded ranking boost so they drift to the top. The boost is
capped so it never overrides intentful results like calculations or keyword
hits. No configuration required.

## Recents on open (session-only)

Open the panel with an empty query and litecast shows what you did recently this
session instead of a blank box:

- Up to 12 recently activated items (apps launched, commands run, conversions
  and web searches copied, emoji, etc.), tagged **Recent**. Selecting one
  re-runs the same action.
- The **last AI interaction** is pinned on top as a `Last AI: …` row whose
  subtitle previews the answer. Selecting it reopens that answer and re-enters
  the follow-up chat thread, so you can keep asking.

This list is in-memory only — it is never written to disk and resets when
litecast quits. It appears only in the normal launcher (not in screenshot mode
or while an AI chat is active). Start typing to return to live results.

## Calculator

Type any arithmetic expression (must contain an operator): `12 * (3 + 4)`,
`2^10`, `100 / 7`. `Enter` copies the result.

## Unit & currency conversion

Natural forms: `<amount> <from> in|to <to>`.

- Units (offline): length, mass, volume, data, speed, time, and temperature.
  Examples: `10 km in mi`, `5 ft to cm`, `2 gb in mib`, `100 f to c`,
  `90 kph to mph`.
- Currency (network, cached): `100 usd to eur`, `50 gbp in jpy`. Rates come from
  `open.er-api.com` and `frankfurter.app` (one picked at random per refresh,
  with fallback to the other), cached to `currency.json`. Offline, the last
  cached rates are used; with no cache it reports that rates are unavailable.

```toml
[conversion]
currency_ttl_hours = 12   # how long cached rates are reused before refreshing
```

## System commands

Fuzzy-search by name: `Lock Screen`, `Sleep`, `Sleep Displays`,
`Toggle Dark Mode`, `Empty Trash`, `Restart`, `Shut Down`, `Toggle Wi-Fi`, and
`Toggle Bluetooth` (only when `blueutil` is installed). Destructive commands
(Empty Trash, Restart, Shut Down) require a second `Enter` to confirm.

Permissions: lock/sleep/Wi-Fi need none; dark mode/trash/restart/shutdown use
AppleScript and prompt for **Automation** on first use.

## Emoji & symbol picker

Trigger with the `emoji` keyword or a `:` prefix: `emoji fire`, `:heart`,
`emoji arrow`. Fuzzy-matches names and keywords across a curated set of common
emoji plus math/arrow/currency symbols. `Enter` copies the glyph.

## Text snippets

List with `snip` (or `snip <filter>`), or surface a snippet directly via its own
keyword. `Enter` copies the expanded text. Placeholders expanded on activation:
`{date}`, `{time}`, `{clipboard}`, `{cursor}` (removed).

```toml
[[snippets.entries]]
keyword = "addr"
name = "Home address"
text = "1 Main St, Springfield"
paste = false             # paste-on-Enter copies to the clipboard
```

## Quicklinks

Parameterized URLs opened in the browser. Trigger with the keyword plus an
argument (URL-encoded into `{query}`), or fuzzy-match the name to open with no
argument.

```toml
[[quicklinks]]
name = "GitHub repo"
keyword = "ghr"
url = "https://github.com/{query}"
```

## Clipboard history

`clip` lists recent clipboard entries; `clip foo` fuzzy-filters (pinned and
unpinned). Entries are typed:

- **Text** - `Enter` copies it back to the clipboard.
- **Link** (http/https) - `Enter` opens it in the browser.
- **Image** - copied images are captured and stored under the support dir
  (`clip-images/`); the row shows a thumbnail and `Enter` re-copies the image.

**Pinning:** each row shows a number. `clip pin <n>` pins entry `n` (it moves to
the top, is marked `[pin]`, and survives eviction); `clip unpin <n>` removes the
pin. Configure image capture and caps in `[clipboard]`:

```toml
[clipboard]
keep_images = true
max_images = 20
```

## Bookmarks & history

Search Chromium-family browsers (Chrome, Brave, Edge, Chromium, Vivaldi):

- `bm <query>` - fuzzy-search bookmarks (parsed from each profile's `Bookmarks`
  JSON; no permission needed).
- `hist <query>` - fuzzy-search browser history (read via the system `sqlite3`;
  the locked DB is copied first). Cached for a few minutes.

`Enter` opens the URL. Both keyword-gated, so nothing touches disk unprompted.
Safari is not supported (its data requires Full Disk Access).

## AI

`? <question>` asks the configured backend (only on `Enter`). `Option+Shift+Space`
captures a screen region to ask about it (vision).

### Follow-up chat

After an answer, the panel enters chat mode (the placeholder shows "Follow up,
or press Esc to exit chat..."). Keep typing and press `Enter` to continue the
conversation with full prior context; the latest answer stays visible for
reference. `Esc` exits chat. The transcript resets when you dismiss the panel or
start a fresh `?` question.

### Quick AI commands

Act on a typed argument, or with no argument on the latest clipboard text:

| Keyword(s) | Action |
| --- | --- |
| `translate`, `tr` | Translate to English |
| `summarize`, `sum` | Summarize |
| `fixgrammar`, `fix` | Fix spelling/grammar |
| `improve`, `rewrite` | Improve writing |

Example: `fix this sentance has typos` or just `summarize` (uses the clipboard).
These are also fuzzy-discoverable by name and reuse the AI flow (answers are
copyable, and follow-up chat continues from them).

### Providers

Set in the `[ai]` config section via `provider`:

| `provider` | Backend | Notes |
| --- | --- | --- |
| `anthropic` | Anthropic Claude | Messages API |
| `openai` | OpenAI | chat-completions |
| `gemini` | Google Gemini | `generateContent`; `google` is an alias |
| `openai-compatible` | any OpenAI-compatible endpoint | set `endpoint`; `cursor` is a legacy alias |

Gemini example (default model, no endpoint needed):

```toml
[ai]
provider = "gemini"
model = "gemini-2.5-flash"
endpoint = ""
```

Get a Gemini key at Google AI Studio (https://aistudio.google.com). Gemini sends
the key in the `x-goog-api-key` header (never the URL). Gemini can also be used
through its OpenAI-compatible endpoint with `provider = "openai-compatible"` and
the matching `endpoint`. A non-empty `endpoint` overrides the default base URL
for `gemini` and `openai-compatible` (useful for proxies).

### Keys

`setkey <api-key>` stores the key in the macOS Keychain (service `litecast`)
under the **active** provider's name. Keys are never written to config files. To
switch providers, change `provider`, then run `setkey` with that provider's key.
