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
  and **Shift+Tab** to cycle backward. The active filter shows as a small pill in
  the search area.

Both drive the same active filter. **Esc** clears an active filter first, then
(when already on `All`) closes the panel. Tokens and the categories they map to:

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

`clip` lists recent clipboard entries; `clip foo` filters. `Enter` copies the
entry back to the clipboard.

## AI

`? <question>` asks the configured backend (only on `Enter`). `setkey <api-key>`
stores the key in the Keychain. `Option+Shift+Space` captures a screen region to
ask about it.
