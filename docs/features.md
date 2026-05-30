# litecast features & keywords

Quick reference for the built-in providers, their triggers, and the related
`config.toml` sections. All config fields use `serde(default)`, so adding any of
these to an existing config is backward compatible.

## Search filters (scope results to a category)

Scope results to a single category three interchangeable ways:

- **Clickable chips:** a horizontal row of category chips
  (`All  Apps  Files  Clipboard  Calc  Web  Commands  Emoji  AI`) sits in its own
  band directly under the search field. Click a chip to activate that filter; the
  active chip is highlighted as an accent pill, the rest are subtle but clearly
  interactive. Clicking re-runs the query and returns focus to the search field.
- **Typed prefix:** start the query with an `@token` followed by a space:
  `@apps safari`, `@files report`, `@calc 10 km in mi`, `@web rust`,
  `@cmd lock`, `@emoji fire`, `@ai explain x`. `@clip` (and any token alone)
  scopes with an empty query.
- **Tab cycle:** press **Tab** to move the highlight forward along the chip row
  (`All -> Apps -> Files -> Clipboard -> Calc -> Web -> Commands -> Emoji -> AI -> All`)
  and **Shift+Tab** to move backward.

While you are still typing an `@token` (before the first space), litecast shows
an autocomplete list of the available shortcuts (these category filters plus any
app commands). The nearest match appears as an inline ghost in the field; press
**Tab** to accept it. Once an `@token` is being completed, **Tab** accepts the
suggestion; otherwise **Tab** cycles the filter as below.

All three drive the same active filter and stay in sync: clicking a chip, typing
a prefix, and Tab-cycling each update the highlighted chip and re-run the query.
**Esc** exits AI chat first (if active), then clears an active filter, then
closes the panel. Tokens and the categories they map to:

| Token | Category | Includes (source labels) |
| --- | --- | --- |
| `@apps` | Apps | App |
| `@files` | Files | File |
| `@clip` | Clipboard | Clip |
| `@calc` | Calc / Conversions | Calc, Convert, Dev, Color, Time |
| `@web` | Web | Web |
| `@cmd` | Commands | Command, Quicklink, Snippet, System, Plugin, Proc, Window, Calendar, Reminders, Network, Notes, Dictionary, Media |
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

## Developer tools

Keyword-gated utilities; `Enter` copies the output to the clipboard. Everything
is hand-rolled (or uses `serde_json` for JSON), so there is no network and no
new dependency.

| Keyword(s) | Example | Result |
| --- | --- | --- |
| `base64` / `b64` | `base64 hello` | Base64-encode the text |
| `base64 decode` / `base64d` | `base64d aGVsbG8=` | Base64-decode |
| `urlencode` / `urldecode` | `urlencode a b&c` | Percent-encode / decode |
| `md5`, `sha1`, `sha256` | `sha256 hello` | Hex digest of the text |
| `uuid` | `uuid` | Random UUID v4 |
| `password <len>` / `pass` | `password 24` | Random password (default 20, 4–256) |
| `lorem <n>` | `lorem 30` | Lorem-ipsum (n words, default 40) |
| `json <text>` | `json {"a":1}` | Pretty-print JSON |
| `jsonmin <text>` | `jsonmin { "a": 1 }` | Minify JSON |

## Color, number-base & timestamp converters

- **Color:** `#RRGGBB`, `#RGB`, `0xRRGGBB`, `rgb(r,g,b)`, or a named color
  (`red`, `navy`, `teal`, …). Shows a swatch (a tiny BMP rendered under the
  support dir) plus the HEX / RGB / HSL representations; `Enter` copies HEX (the
  RGB and HSL rows copy their own value). Explicit forms rank high; bare color
  names rank modestly so they never bury app results.
- **Number base:** `<number> to dec|hex|bin|oct`. The input may be decimal or
  `0x` / `0b` / `0o` prefixed: `0x1f to dec`, `255 to hex`, `0b1010 to oct`.
- **Epoch / timestamp:** `epoch <n>` (or `timestamp`/`unix`) converts a Unix time
  to local + UTC (13-digit values are treated as milliseconds); `now epoch`
  prints the current Unix time. Uses the built-in `date` CLI for correct local
  time and DST.

## Date & time

- **World clock:** `time in <place>` for common cities (`time in Tokyo`,
  `time in London`) and zone abbreviations (`time in IST`, `time in PST`,
  `time in UTC`). Uses the built-in `date` CLI with a `TZ` override, so DST is
  handled correctly. Add your own named zones in `[datetime]` (below).
- **Date math:** `days until 25 Dec`, `days since 2020-01-01`, `today+30d`,
  `now-2w`. Hand-rolled with a civil-days algorithm (no `chrono`).
- **Timers:** `timer 5m`, `timer 30s`, `timer 1h30m`, optionally with a label
  (`timer 10m tea`). `Enter` starts a detached `sleep` that fires an
  `osascript` notification when elapsed — it never blocks the UI.

```toml
[[datetime.timezones]]
name = "HQ"
tz = "America/New_York"     # IANA identifier; used by "time in HQ"
```

## System commands

Fuzzy-search by name. Built-ins:

- **Power/session:** `Lock Screen`, `Sleep`, `Sleep Displays`, `Empty Trash`,
  `Restart`, `Shut Down` (the last three require a second `Enter` to confirm).
- **Appearance:** `Toggle Dark Mode`.
- **Volume:** `Volume Up`, `Volume Down`, `Mute`, `Unmute`, and `volume <0-100>`
  / `set volume <n>` to set a level (via `set volume`).
- **Wi-Fi:** `Toggle Wi-Fi`, `Wi-Fi On`, `Wi-Fi Off` (via `networksetup`).
- **Bluetooth:** `Toggle Bluetooth`, `Bluetooth On/Off` — only when the optional
  `blueutil` helper is installed (no permission-free CLI otherwise).
- **Brightness:** `Brightness Up/Down` and `brightness <0-100>` — only when the
  optional `brightness` helper is installed.
- **Caffeinate:** `Caffeinate` keeps the Mac awake (spawns `caffeinate`);
  `Decaffeinate` stops it.
- **Disks:** `Eject All Disks`.
- **Focus:** `Toggle Do Not Disturb` — best-effort; runs a Shortcut named
  "Toggle Do Not Disturb" if you have created one (modern macOS has no stable
  scriptable Focus API), and degrades to a no-op otherwise.

Permissions: lock/sleep/volume/Wi-Fi/caffeinate need none; dark mode / trash /
restart / shutdown / eject use AppleScript and prompt for **Automation** on
first use. Bluetooth and brightness degrade gracefully when their helper CLIs
are absent.

## File power actions & recent files

Keyword-gated, so the disk is only scanned on demand:

| Keyword | Effect |
| --- | --- |
| `recent` | Recently modified files across Desktop / Downloads / Documents |
| `downloads` / `dls` | Newest items in `~/Downloads` |
| `reveal <path>` | Reveal in Finder (`open -R`) |
| `ql <path>` / `quicklook` | Quick Look preview (`qlmanage -p`) |
| `copypath <path>` | Copy the POSIX path to the clipboard |
| `folder <path>` | Open the enclosing folder |

Recent/download rows open the file on `Enter`. Paths accept `~` expansion.

## Calendar & reminders

AppleScript bridges to Calendar and Reminders (no entitlement-heavy linking).
macOS prompts for **Automation** permission on first use.

| Keyword | Effect |
| --- | --- |
| `today` / `agenda` | List today's calendar events (cached briefly) |
| `remind <text> [at <time>]` | Quick-add a reminder (`remind buy milk at 5pm`) |
| `event <text> [at <time>]` | Quick-add a 1-hour calendar event (`event Lunch at 1pm`) |

Listing today's events shells out to Calendar (slow), so results are cached for
~2 minutes; create actions only run on `Enter`. Times accept `5pm`, `5:30pm`,
`17:00`, `9am`.

## Network info

Keyword-gated; nothing runs on the default path.

| Keyword | Effect |
| --- | --- |
| `ip` / `localip` | Local IP (`ipconfig getifaddr en0`, with en1/en2 fallback) |
| `myip` / `public ip` | Public IP via a single HTTP GET (no polling), cached ~5 min |
| `ports` / `listening` | Listening TCP ports (`lsof -nP -iTCP -sTCP:LISTEN`) |
| `port <n>` | What's listening on a specific port |
| `wifi networks` / `networks` | Preferred Wi-Fi networks (`networksetup`) |

The public-IP lookup is bounded by a hard 5-second timeout so it can never hang
the worker thread. `Enter` copies the relevant value (IP, port, network name).

## Quick notes

`note <text>` appends a timestamped line to a plain-text notes file; `note` or
`notes` (no argument) opens that file. Optionally mirror each capture into Apple
Notes.

```toml
[notes]
# file = "notes.txt"     # relative -> support dir; or an absolute path
apple_notes = false      # also create an Apple Notes note (asks for Automation)
```

## Dictionary & spell

- `define <word>` shows an inline definition via the macOS **Dictionary
  Services** (through `python3`/PyObjC) when reachable, and always offers a
  "Look up in Dictionary" row that opens Dictionary.app via the `dict://` scheme.
- `spell <word>` checks the system word list (`/usr/share/dict/words`) and, when
  the word isn't found, suggests the nearest matches by edit distance.

No network. Definitions are cached per word; the word list loads once on first
use.

## Media controls

Control the active player (Spotify or Music) via AppleScript when one is
running; the action is a graceful no-op if neither is. Keyword-gated.

| Keyword(s) | Effect |
| --- | --- |
| `play` / `resume`, `pause` | Play / play-pause |
| `next` / `next track` / `skip` | Next track |
| `prev` / `previous` / `back` | Previous track |
| `now playing` / `track` | Show the current track (`Enter` copies it) |

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

## Custom commands & aliases

`[[commands]]` add fuzzy-searchable results. Each has a `name`, a `kind`
(`"open"` for a file/URL/app or `"shell"` for a `sh -c` command), and a
`target`. An optional `keyword` triggers it directly (and, if `target` contains
`{}`, substitutes the text typed after the keyword).

Optional `alias` (one term) and/or `aliases` (a list) are extra search terms
folded into name matching, so a short token surfaces the command without
changing its display name:

```toml
[[commands]]
name = "Open GitHub"
keyword = "gh"
alias = "git"
aliases = ["hub", "repo"]
kind = "open"
target = "https://github.com/{}"
```

## App commands (`@keyword`)

`@`-namespaced actions that take a free-text argument. Type `@` to open the
autocomplete list of available shortcuts (category filters and app commands),
fuzzy-match the keyword, and press **Tab** (or **Enter**) to accept the nearest
match. The nearest match is also shown as an inline ghost in the search field.
After accepting, type an argument and press **Enter** to run.

Built-ins (no AI, no config required):

| Keyword | Action |
| --- | --- |
| `@term <command>` | Open Terminal.app and run the command (empty = just open Terminal) |
| `@finder <path>` | Open a path/folder in Finder (empty = home) |
| `@safari <url-or-query>` | Open a URL, or web-search the text, in your default browser |

Define your own in `[[app_commands]]`. `{query}` (or `{arg}`) is replaced with
the text typed after the keyword; a user entry reusing a built-in keyword
overrides it.

```toml
[[app_commands]]
keyword = "ed"
name = "Edit in editor"
kind = "shell"          # "terminal" | "shell" | "applescript" | "open"
template = "code {query}"
```

## Quicklinks

Parameterized URLs opened in the browser. Trigger with the keyword plus an
argument (URL-encoded into `{query}`), or fuzzy-match the name (or any `alias`)
to open with no argument.

```toml
[[quicklinks]]
name = "GitHub repo"
keyword = "ghr"
alias = "repo"
url = "https://github.com/{query}"
```

## Custom global hotkeys

`[[hotkeys]]` register additional system-wide hotkeys alongside the built-in
`Option+Space` (toggle) and `Option+Shift+Space` (screenshot). Each binds a key
combo to an action that fires directly, without opening the panel.

**Combo syntax:** modifiers and a key joined by `+`, e.g. `Cmd+Shift+S`.

- Modifiers: `Cmd` (aliases `Command`/`Super`/`Win`/`Meta`), `Ctrl`
  (`Control`), `Alt` (`Option`/`Opt`), `Shift`. At least one is required.
- Key: a letter `A`-`Z`, a digit `0`-`9`, `F1`-`F12`, `Space`, `Enter`, `Tab`,
  `Esc`, an arrow (`Up`/`Down`/`Left`/`Right`), or common punctuation
  (`,` `.` `/` `-` `=` `;` `'` `` ` `` `\` `[` `]`).

**Action kinds:**

| `kind` | `target` is | Effect |
| --- | --- | --- |
| `open` | a URL / path / app | opened via `open` |
| `shell` | a shell command | run via `sh -c` |
| `command` | the `name` of a `[[commands]]` entry | runs that command's action |

```toml
[[hotkeys]]
key = "Cmd+Shift+G"
kind = "open"
target = "https://github.com"

[[hotkeys]]
key = "Ctrl+Alt+T"
kind = "shell"
target = "open -a Terminal"
```

Registration is best-effort: if a combo is unparseable or already claimed by
another app, litecast logs it and carries on (the rest of the app is unaffected).

## Process manager

Type `kill` or `ps` (optionally with a filter, e.g. `kill safari`) to list your
running processes by name, PID, and %CPU. The provider is keyword-gated, so it
never runs `ps` unprompted. `Enter` arms a two-step confirmation
("Press Enter again to kill <name> (pid …)"); a second `Enter` sends **SIGTERM**
(graceful). Critical system processes (`WindowServer`, `loginwindow`, `Finder`,
litecast itself, …) are hidden to avoid foot-guns. No permissions required;
only your own user's processes are listed.

## Window management (opt-in, needs Accessibility)

**Off by default.** This is the one litecast feature that needs the macOS
**Accessibility** permission, so it is gated behind config and stays inert until
you opt in:

```toml
[window]
enabled = true
```

With it enabled, type `win` (e.g. `win left`, `win max`) to move/resize the
**frontmost app's** focused window: Left/Right/Top/Bottom Half, Left/Right
Third, Center Two-Thirds, Maximize, Center, and Next/Previous Display.

The first time you run a window command, macOS prompts you to grant litecast
access under **System Settings › Privacy & Security › Accessibility**. Nothing
runs or prompts until you both enable the section and trigger a command; if
access is denied, litecast shows a row that opens the right Settings pane
instead of failing silently. litecast remains fully functional with
Accessibility never granted.

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
