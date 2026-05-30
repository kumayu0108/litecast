# litecast v2 design plan

Goal: add powerful launcher features while keeping litecast lightweight, native
(AppKit via `objc2`), keyboard-first, and dependency-minimal.

This document is grounded in the current architecture:

- **Providers** (`src/engine.rs`): `Provider::query(&self, query, &mut Vec<Item>)`
  is called on *every keystroke* on a single background worker thread (queries
  are coalesced; `Provider` must be `Send + Sync`). `Engine::query` runs all
  providers, sorts by `score` descending, truncates to `max_results` (8).
- **Model** (`src/model.rs`): `Item { title, subtitle, action, score: i64,
  source: &'static str, icon }` and `Action { Open, RunShell, CopyText, AskAi,
  SetApiKey, None }`. `Action::execute() -> bool` returns whether to close the
  panel. `AskAi` is special-cased in the UI (runs async, keeps panel open).
- **UI** (`src/main.rs`): the `AppDelegate` owns an `NSPanel`, dispatches
  queries via an `mpsc` channel to the worker, applies results on the main
  thread (`performSelectorOnMainThread`), and routes Enter through
  `activate_selection`. A 1s `NSTimer` watches the pasteboard; `global-hotkey`
  drives Option+Space / Option+Shift+Space.
- **Config** (`src/config.rs`): TOML with `serde(default)`, holding
  `web_search_url`, `commands`, `ai`, `ui`. Secrets live in the Keychain
  (`src/secrets.rs`). AI HTTP uses blocking `ureq` (`src/ai.rs`).

Design principles carried into v2: no per-keystroke process spawning unless a
keyword is matched (see `plugins`/`commands`); all slow or network work happens
on the worker or a spawned thread; keep the release binary small (current
profile: `lto`, `strip`, `panic = "abort"`).

---

## 0. Build order / phasing (impact vs. effort)

**Phase 1 — high impact, low risk, zero new permissions, zero/tiny deps.**
1. Smarter ranking / frecency (#9) — multiplies the value of *every* other
   feature; touches `Engine` once. **(M)**
2. System commands (#2) — pure `RunShell` (`osascript`/`pmset`/`networksetup`).
   Huge perceived power for little code. **(S)**
3. Snippets / text expansion (#3, paste-on-Enter variant) — reuses `CopyText`.
   **(S)**
4. Emoji & symbol picker (#4) — bundled dataset + `CopyText`. **(S/M)**
5. Unit conversion (#7, units only; currency deferred to Phase 2). **(M)**
6. Quicklinks (#5, the `{query}` part; bookmarks/history deferred). **(S)**

**Phase 2 — high impact, moderate effort or one new dep / network source.**
7. AI upgrades: follow-up chat + quick AI commands (#6). **(M)**
8. Clipboard upgrades: pin, image/link types, fuzzy search (#8). **(M)**
9. Currency conversion (#7 remainder) — network + cache. **(S/M)**
10. Browser bookmark/history search (#5 remainder) — file/SQLite + FDA. **(M)**

**Phase 3 — powerful but with the biggest permission/safety footprint.**
11. Window management (#1) — **requires Accessibility**; biggest stance shift. **(L)**
12. Process manager (#10) — listing is easy; killing needs confirmations. **(M)**

Rationale: Phase 1 keeps litecast's "no special permissions" promise intact and
ships the broadest wins. Accessibility-gated features (window management; inline
snippet expansion; synthetic paste) are deliberately last so the app stays
usable and trustworthy without ever granting AX.

---

## 1. Consolidated new permissions

| Feature | macOS permission | Trigger |
| --- | --- | --- |
| Window management (#1) | **Accessibility (AX)** — `AXIsProcessTrustedWithOptions` | required to move/resize other apps' windows |
| Inline snippet expansion (#3, optional) | **Accessibility** (CGEventTap + synthetic keys) | only if we do live expansion instead of paste-on-Enter |
| Synthetic Cmd+V paste (#3/#4, optional) | **Accessibility** (CGEventPost) | only if "paste directly" instead of "copy to clipboard" |
| Dark-mode toggle, empty trash, restart/shutdown, sleep (#2) | **Automation (Apple Events)** TCC prompt for "System Events"/"Finder" | first time `osascript` controls those apps |
| Safari bookmarks/history (#5) | **Full Disk Access** (TCC) | reading `~/Library/Safari/*` |
| Lock screen, display sleep, Wi-Fi (#2) | none (CGSession / `pmset` / `networksetup`) | — |
| Chrome bookmarks (#5) | none (plain JSON in user dir) | — |
| Process listing/kill of *own-user* processes (#10) | none (`ps`, `kill`) | killing other users' procs would need privileges (out of scope) |

Everything in **Phase 1** needs **no new permission**. Phase 2 adds the
Automation prompt (only on first use of those specific commands) and optionally
Full Disk Access (Safari only). Phase 3 introduces the Accessibility prompt.

**Stance note:** litecast today advertises "no Accessibility permission needed."
Window management breaks that. Recommendation: keep AX strictly opt-in and
lazy — never call `AXIsProcessTrustedWithOptions({prompt: true})` until the user
actually activates a window-management item, and gate the whole provider behind
a config flag (`[window] enabled = false` by default). The app must remain fully
functional with AX denied.

---

## 2. Consolidated new dependencies

Preference order: native API > hand-rolled > tiny crate > shelling out to a
system binary > heavy crate. Current deps: `global-hotkey`, `keyring`,
`nucleo-matcher`, `objc2(+app-kit/foundation)`, `serde`, `serde_json`, `toml`,
`ureq`.

| Need | Recommendation | Why |
| --- | --- | --- |
| AX window control (#1) | **`accessibility-sys`** (raw `AXUIElement*` FFI) *or* hand-rolled `extern "C"` bindings | `accessibility-sys` is a thin `-sys` shim (no runtime weight); hand-rolling ~8 functions is also viable and adds zero deps. Avoid the higher-level `accessibility` crate (more surface than needed). |
| Synthetic key events (#3/#4 optional, #1 helpers) | **`core-graphics`/`core-graphics-types`** or hand-rolled `CGEvent*` FFI | only if we implement synthetic paste; otherwise skip entirely. |
| Emoji dataset (#4) | **bundled, generated static table** (no crate) | compile a compact `&[(char, &str, &[&str])]` from a CLDR/`emoji-test.txt` snapshot via `build.rs`. ~100-200 KB in-binary, zero runtime dep, full control over size. Prefer this over the `emojis`/`unic-emoji` crates. |
| SQLite (Chrome/Safari history, #5) | **shell out to `/usr/bin/sqlite3`** | avoids `rusqlite` (bundles libsqlite, +~1 MB, build complexity). `sqlite3` ships with macOS. JSON bookmarks need no SQLite at all. |
| Binary plist (Safari bookmarks, #5) | shell out: `plutil -convert json -o - <file>` then `serde_json` | no `plist` crate needed. |
| Bluetooth toggle (#2) | **none preferred**; optional `blueutil` (external binary, user-installed) | macOS has no permission-free Bluetooth CLI. Document `blueutil` as optional; do not bundle. Wi-Fi uses built-in `networksetup`. |
| Currency rates (#7) | reuse **`ureq`** + `serde_json` | already present; just add a cache file. |
| Frecency / usage store (#9) | reuse **`serde_json`** | new `usage.json` in the support dir. |

Net new crates if all phases ship: at most `accessibility-sys` (Phase 3) and,
only if synthetic paste is chosen, `core-graphics`. **Phase 1 adds no crates.**

---

## 3. Cross-cutting changes (design once, share across features)

These are the shared primitives. Designing them coherently up front avoids
churn later. (Note: `Item` is already gaining an `icon` field from concurrent
work; the additions below are additive and compatible.)

### 3a. `Action` additions

```rust
pub enum Action {
    // ...existing: Open, RunShell, CopyText, AskAi, SetApiKey, None

    /// Copy `text` then (if AX granted) synthesize Cmd+V. Falls back to plain
    /// copy when AX is denied. Used by snippets/emoji "paste" variants.
    Paste(String),

    /// Copy a file's contents to the pasteboard as a typed object (e.g. PNG
    /// image). Used by clipboard image entries. `kind` selects pasteboard type.
    CopyFile { path: String, kind: ClipKind },

    /// Two-step confirmation wrapper for destructive actions (empty trash,
    /// shutdown, kill PID). UI shows "Press Enter again to confirm".
    Confirm { label: String, inner: Box<Action> },

    /// Window-management operation against the frontmost app's focused window.
    /// Handled specially in the UI (needs main thread + AX), like AskAi.
    Window(WindowOp),

    /// Continue an AI conversation: append `prompt`, keep panel open, re-render
    /// transcript. Carries the running message history (see #6).
    AskAiFollowup { prompt: String },
}
```

`execute()` keeps returning `bool` (close panel?). `Window`, `AskAiFollowup`,
and `Confirm` are special-cased in `activate_selection` exactly like `AskAi` is
today (they return `false`/keep-open or manage their own lifecycle). `Paste`,
`CopyFile` execute inline and close.

### 3b. `Item` additions

```rust
pub struct Item {
    // ...existing: title, subtitle, action, score, source, icon

    /// Stable identity for frecency + custom hotkeys. e.g. "app:/Applications/Safari.app".
    /// Defaults to None (item not frecency-tracked, e.g. calc results).
    pub id: Option<String>,
}
```

Add a builder `with_id(id)` mirroring the existing `with_icon`. Providers set a
stable `id` only for things worth learning (apps, files, commands, quicklinks,
snippets). Volatile results (calc, AI answers) leave it `None`.

### 3c. `Engine` change — frecency boost (one place)

`Engine` gains an optional `Frecency` handle (loaded from `usage.json`). After
providers fill `out`, before the final sort:

```rust
for item in &mut out {
    if let Some(id) = &item.id {
        item.score += frecency.boost(id); // bounded, e.g. 0..=400
    }
}
out.sort_by(...); // unchanged
```

Boost is bounded so it nudges ties and near-ties (apps/files) but never
overrides intentful high-score results (calc = 10_000, keyword hits = 8_500+).
Usage is *recorded* in `main.rs::activate_selection` (it already has the chosen
`Item`): on activate, if `item.id` is set, call `frecency.record(id)`.

### 3d. Config additions (all `serde(default)`, backward compatible)

```toml
[snippets]                 # #3
# [[snippets.entries]] keyword = "addr", text = "1 Main St", paste = false

[[quicklinks]]             # #5
# name = "GitHub repo", keyword = "ghr", url = "https://github.com/{query}"

[conversion]               # #7
# currency_endpoint = "https://open.er-api.com/v6/latest/{base}"
# currency_ttl_hours = 12

[window]                   # #1
# enabled = false          # AX-gated; off by default

[ai]                       # #6 (extends existing [ai])
# chat = true
# [[ai.commands]] keyword = "tr", name = "Translate", prompt = "Translate to English:\n{input}"

[aliases]                  # #9
# "ss" = "System Preferences"   # alias -> command/app name

[[hotkeys]]                # #9 custom per-item hotkeys
# id = "app:/Applications/Safari.app", key = "Cmd+Shift+S"

[clipboard]                # #8
# keep_images = true, max_image_mb = 5
```

### 3e. Worker-thread invariants (apply to every new provider)

- No process spawn / network / disk-walk unless a **keyword prefix** matched
  (follow `commands`/`plugins`/`clip`). Conversions/snippets/emoji are pure-CPU
  and can run unconditionally but should bail fast on non-matching input.
- Anything that can block (currency fetch, bookmark SQLite, AX calls) must run
  off the worker — either cached and refreshed on a timer, or spawned like the
  AI flow, never inline in `query`.

---

## 4. Window management (#1) — **L**

**(a) UX.** Keyword `win` (or `w`) lists ops: `Left Half`, `Right Half`,
`Top/Bottom Half`, `Maximize`, `Center`, `Left/Center/Right Third`, `Next
Display`, `Restore`. Typing `win left` filters. Enter applies to the *frontmost
app's focused window* and closes the panel. Eventually bind common ops to custom
hotkeys (#9) for no-panel use.

**(b) Model mapping.** New `Action::Window(WindowOp)` where
`WindowOp { kind: Snap|Move|Resize|Display, target: Region }`. Special-cased in
`activate_selection` (must run on main thread; AX calls are main-thread-safe and
need the trust check first). Items carry `source = "Window"`, fixed high score
when keyword-triggered.

**(c) Implementation.** Accessibility API:
`AXUIElementCreateApplication(frontmost_pid)` →
`AXUIElementCopyAttributeValue(kAXFocusedWindowAttribute)` →
`AXUIElementSetAttributeValue(kAXPositionAttribute / kAXSizeAttribute)` using
`AXValueCreate(kAXValueCGPointType / kAXValueCGSizeType, ...)`. Frontmost PID via
`NSWorkspace.frontmostApplication` (already linked). Screen geometry via
`NSScreen` (already used in `layout`): use `visibleFrame` per screen, and map AX
top-left coordinates (origin top-left, y-down) vs. Cocoa bottom-left — handle the
flip explicitly. Multi-display: enumerate `NSScreen.screens`; "Next Display"
moves the window to the next screen's `visibleFrame` preserving relative size.
Use `accessibility-sys` or ~8 hand-rolled `extern "C"` decls.

**(d) Permissions.** **Accessibility (AX)** — the major stance change. Gate
behind `[window] enabled`. Only call `AXIsProcessTrustedWithOptions` with the
prompt option when the user first activates a window item; if untrusted, show an
item that opens `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`.

**(e) Tradeoffs/risks.** Loses the "no AX" selling point — call this out to
users in README and make it opt-in. Some apps report stale/clamped frames
(min-size windows, full-screen spaces); fail gracefully (no-op + subtle error
row). Coordinate-flip bugs are the classic pitfall. Cheap at runtime (a few AX
calls per activation; nothing per keystroke).

**(f) Complexity: L** (AX FFI + coordinate math + multi-display + permission UX).

---

## 5. System commands (#2) — **S**

**(a) UX.** Each is a named result, fuzzy-searchable, optionally keyworded:
`Lock Screen`, `Sleep`, `Sleep Displays`, `Empty Trash`, `Toggle Dark Mode`,
`Restart`, `Shut Down`, `Toggle Wi-Fi`, `Toggle Bluetooth`. Enter runs it;
destructive ones (Empty Trash, Restart, Shut Down) require a second Enter via
`Action::Confirm`.

**(b) Model mapping.** Almost all map to existing `Action::RunShell`. Destructive
ones wrap in `Action::Confirm { inner: RunShell(...) }`. New `SystemProvider`
holds a static table of `(name, keyword, Action)`; fuzzy-match names with
`fuzzy_score`, like `commands`. `id` set for frecency. No new core types beyond
`Confirm`.

**(c) Implementation (prefer permission-free).**
- Lock screen: `/System/Library/CoreServices/"Menu Extras"/User.menu/Contents/Resources/CGSession -suspend` (no permission).
- Sleep system: `pmset sleepnow`. Sleep displays: `pmset displaysleepnow`. (no permission)
- Wi-Fi: `networksetup -setairportpower en0 off|on` (detect device via `networksetup -listallhardwareports`). (no permission)
- Empty Trash: `osascript -e 'tell application "Finder" to empty trash'` (Automation prompt).
- Toggle Dark Mode: `osascript -e 'tell application "System Events" to tell appearance preferences to set dark mode to not dark mode'` (Automation prompt).
- Restart/Shut Down: `osascript -e 'tell application "System Events" to restart'` / `... to shut down` (Automation prompt).
- Bluetooth: no permission-free CLI. Use `blueutil -p toggle` *if installed*;
  otherwise hide the item (detect `blueutil` on PATH at startup). Avoid private
  `IOBluetooth` frameworks.

**(d) Permissions.** Lock/sleep/Wi-Fi: none. Trash/dark-mode/restart/shutdown:
Automation (Apple Events) TCC prompt on first use of each target app. Bluetooth:
none from us, but requires user-installed `blueutil`.

**(e) Tradeoffs/risks.** AppleScript prompts can confuse first-time users — show
a subtitle like "first run will ask for permission." Hardcoded `en0` is fragile;
detect the Wi-Fi port. Keep commands in a static table (cheap, no per-keystroke
cost beyond fuzzy matching a handful of strings).

**(f) Complexity: S.**

---

## 6. Snippets / text expansion (#3) — **S** (paste-on-Enter); **L** (inline)

**(a) UX.** Keyword `snip` lists snippets; `snip addr` filters; or a snippet's
own keyword (e.g. `addr`) surfaces it directly. Enter pastes the expanded text.
Support simple placeholders: `{date}`, `{time}`, `{clipboard}`, `{cursor}`.

**(b) Model mapping.** `SnippetsProvider` reads `[snippets]` config (or a
separate `snippets.toml`). Each snippet → `Item` with `Action::Paste(text)` (or
`CopyText` if `paste = false`). `Paste` = copy + optional synthetic Cmd+V.

**(c) Implementation.**
- **Storage:** TOML under config (small) or a dedicated `snippets.toml` in the
  support dir (better for large/multi-line snippets; hot-reload on file mtime).
- **Paste-on-Enter (recommended, no permission):** put text on the pasteboard
  via existing `set_clipboard`; user presses Cmd+V. Optionally auto-press Cmd+V
  via `CGEventPost` — but that **requires Accessibility**, so make it the
  fallback inside `Action::Paste` (synthesize if trusted, else just copy).
- **Live inline expansion (abbreviation → text as you type anywhere):** needs a
  global `CGEventTap` keystroke monitor + synthetic backspaces/insertion. This
  requires **Accessibility** and is intrusive/fragile. **Recommendation: do not
  ship inline expansion in v2**; offer paste-on-Enter only. Revisit as an
  opt-in Phase 3 add-on if demanded.

**(d) Permissions.** Paste-on-Enter (copy only): none. Auto Cmd+V or inline
expansion: Accessibility.

**(e) Tradeoffs/risks.** Inline expansion is where text-expansion tools spend
real engineering (per-app quirks, undo, password fields); not worth the AX cost
and risk for v2. Placeholders keep it cheap and useful. `{clipboard}` reads the
current pasteboard at activation time.

**(f) Complexity: S** for paste-on-Enter, **L** if inline expansion is attempted.

---

## 7. Emoji & symbol picker (#4) — **S/M**

**(a) UX.** Keyword `emoji` (alias `:`): `emoji fire`, `:fire`, `:heart`. Fuzzy
match over names + keywords; rows show the glyph + name. Enter copies the glyph
(`CopyText`), or pastes (`Action::Paste`) if the user prefers. Include math/
currency/arrow symbols too (`emoji arrow` → → ⇒ ↦ …).

**(b) Model mapping.** `EmojiProvider` keyword-gated. Each match →
`Item { title: "😀  grinning face", action: CopyText("😀") }`, `source = "Emoji"`.
Reuse `fuzzy_score`. No new core types.

**(c) Implementation.** **Bundle a generated static dataset** — do *not* fetch
or add a heavyweight crate. A `build.rs` reads a checked-in compact data file
(derived from Unicode `emoji-test.txt` + CLDR short names/keywords) and emits a
`static EMOJI: &[(char, &str, &[&str])]`. Full set (~3.7k emoji) with names +
a few keywords is ~100-200 KB in the binary — acceptable given the current tiny
footprint, and far smaller than pulling an emoji crate's data + code. To stay
lean, optionally ship a curated ~1k "common" subset. Symbols are a second small
hand-written table.

**(d) Permissions.** None.

**(e) Tradeoffs/risks.** Binary-size vs. coverage tradeoff — mitigate by
curating the list and storing keywords compactly (e.g. join with `|`). Searching
3.7k short strings per keystroke is trivial for `nucleo`. Skin-tone variants:
keep base emoji only in v2 to control size.

**(f) Complexity: S/M** (mostly the build-time dataset generation).

---

## 8. Quicklinks + browser bookmark/history search (#5) — **S** (quicklinks) / **M** (browser)

**(a) UX.**
- **Quicklinks:** parameterized URLs. `[[quicklinks]]` with `keyword` + `url`
  containing `{query}`. `ghr rust-lang/rust` → opens the templated URL. Without
  an arg, surfaces by fuzzy name match.
- **Bookmarks/history:** keyword `bm` (bookmarks) and `hist` (history). Fuzzy
  search titles/URLs; Enter opens (`Action::Open(url)`).

**(b) Model mapping.** Quicklinks are essentially the existing `commands`
mechanism specialized to URLs — could reuse `CommandsProvider` semantics but a
dedicated `[[quicklinks]]` section reads cleaner. Bookmarks/history →
`BookmarksProvider` producing `Action::Open(url)` items, `source = "Bookmark"`/
`"History"`, with `id` for frecency.

**(c) Implementation.**
- Quicklinks: trivial `{query}` substitution + `percent_encode` (already in
  `websearch.rs`); `Action::Open`.
- **Chrome bookmarks:** `~/Library/Application Support/Google/Chrome/Default/Bookmarks`
  is plain JSON → parse with existing `serde_json`. **No permission, no SQLite.**
- **Chrome history:** `.../Default/History` is SQLite and *locked while Chrome
  runs*. Copy the file to a temp path, then `sqlite3 <copy> "SELECT url,title
  FROM urls ORDER BY last_visit_time DESC LIMIT N"`. Shell out to `/usr/bin/
  sqlite3` (no `rusqlite` dep). Run off the worker / cache results.
- **Safari bookmarks:** `~/Library/Safari/Bookmarks.plist` (binary plist) →
  `plutil -convert json -o - <file>` then `serde_json`. **History:**
  `~/Library/Safari/History.db` via `sqlite3`. The `~/Library/Safari` dir is
  TCC-protected → **Full Disk Access required**.
- Cache parsed bookmarks at startup + refresh on mtime change; never parse per
  keystroke.

**(d) Permissions.** Quicklinks + Chrome bookmarks: none. Chrome history: none
(file is user-readable; copy avoids lock). Safari anything: **Full Disk Access**.

**(e) Tradeoffs/risks.** Safari's FDA requirement is heavy — make Safari sources
opt-in and clearly labeled; ship Chrome + quicklinks first. Multiple Chrome
profiles (`Default`, `Profile 1`…) — enumerate profile dirs. History DBs can be
large — `LIMIT` the query and cache. Copying the locked DB is the standard,
safe approach.

**(f) Complexity: S** (quicklinks), **M** (browser integration + caching + FDA).

---

## 9. AI upgrades (#6) — **M**

**(a) UX.**
- **Follow-up chat:** after a `?`-question answer, the panel stays open;
  continuing to type + Enter sends a follow-up that includes prior turns. A
  transcript renders as result rows; `Esc` exits chat. (Builds on today's
  already-async, panel-stays-open AI flow.)
- **Quick AI commands:** configurable keywords `tr` (translate), `sum`
  (summarize), `fix` (fix grammar) operating on `{clipboard}` or a typed arg:
  `fix this sentence has a typo` or `tr` (uses clipboard). Enter sends; answer
  copyable via existing `answer_to_items`.

**(b) Model mapping.** Add `Action::AskAiFollowup { prompt }`; store the running
`Vec<(role, content)>` transcript in the `AppDelegate` ivars (new
`RefCell<Vec<ChatMsg>>`). Quick commands are `AskAi`/`AskAiFollowup` with a
templated prompt assembled by a new `AiCommandsProvider` reading
`[[ai.commands]]`. `start_ai` already exists; generalize it to accept history.

**(c) Implementation.** Extend `ai::ask` to take a `&[Message]` instead of a
single `prompt` (both Anthropic and OpenAI bodies already use a `messages`
array — minimal change). On each turn, append the user message, call on a
spawned thread (as today), append the assistant reply to the transcript, render.
Quick commands fill `{input}` from clipboard (read pasteboard) or the typed arg.
Reuse `ureq`, `secrets`, the `ai_generation` stale-guard, and
`performSelectorOnMainThread`.

**(d) Permissions.** None (network only; key already in Keychain).

**(e) Tradeoffs/risks.** Token growth across turns — cap transcript length
(e.g. last N turns) and `max_tokens` (already 1024). Keep requests strictly
Enter-triggered (never per keystroke) — the current design already guarantees
this. Rendering multi-turn transcripts in the fixed `NSTableView` needs a clear
visual convention (e.g. prefix rows with `You:`/`AI:`); keep it simple.

**(f) Complexity: M.**

---

## 10. Unit & currency conversion in the calculator (#7) — **M**

**(a) UX.** Natural queries: `10 km in mi`, `100 f to c`, `5 ft to cm`,
`2 gb in mb`, `100 usd to eur`. Result row `= 6.21 mi` with `Enter to copy`,
ranked very high like calc. Works inside or alongside `CalcProvider`.

**(b) Model mapping.** Either extend `CalcProvider` or add a sibling
`ConvertProvider`; output is the same `Item` with `Action::CopyText(result)`,
`source = "Convert"`, high fixed score (~9_500, just under calc's 10_000). No new
core types.

**(c) Implementation.**
- **Units (hand-rolled):** static tables of `(unit, canonical_factor)` per
  dimension (length, mass, volume, speed, data, time) + special-case temperature
  (affine, not multiplicative). Parse `^\s*([\d.]+)\s*(\w+)\s+(?:in|to)\s+
  (\w+)\s*$`. Convert via canonical base. Pure CPU, no deps.
- **Currency (network):** fetch rates from a free, key-less endpoint
  (recommend `https://open.er-api.com/v6/latest/USD` — no API key, generous
  limits; alt: `https://api.frankfurter.app/latest?from=USD`). Cache to
  `currency.json` in the support dir with a fetch timestamp; refresh when older
  than `currency_ttl_hours` (default 12). **Offline behavior:** use the cached
  table and append "(rates from <date>)" to the subtitle; if no cache exists,
  show "Currency rates unavailable offline." Fetch happens on a spawned thread /
  worker, **never inline per keystroke** — kick a refresh at startup and lazily
  when a currency query is seen and the cache is stale.

**(d) Permissions.** Units: none. Currency: network only (no entitlement).

**(e) Tradeoffs/risks.** Endpoint availability/format drift — isolate parsing
and degrade gracefully to cache. Ambiguous unit tokens (`m` = meter vs. mile?) —
prefer explicit tokens and document. Keep currency strictly cached so typing
stays instant.

**(f) Complexity: M** (units S, currency caching/offline is the bulk).

---

## 11. Clipboard upgrades (#8) — **M**

**(a) UX.** Extend the existing `clip` keyword: pinned entries appear first;
`clip foo` fuzzy-searches (already supported); image entries show `[image]` with
a thumbnail (via the `icon` field); link entries are detected and offer
`Open`/`Copy`. A modifier or sub-keyword (`clip pin`) toggles a pin. Enter on an
image copies the image back to the pasteboard; on a link, Enter copies (Cmd-Enter
opens).

**(b) Model mapping.** Generalize `History` from `VecDeque<String>` to
`VecDeque<ClipEntry>`:

```rust
struct ClipEntry { kind: ClipKind /*Text|Image|Link*/, text: String,
                   path: Option<String> /*image file*/, pinned: bool, ts: u64 }
```

Image entries use new `Action::CopyFile { path, kind: Image }`; text/link use
`CopyText`. Pinned entries get a score boost in `ClipboardProvider`.

**(c) Implementation.**
- **Persistence format change:** `clipboard.json` becomes `Vec<ClipEntry>`.
  Add a one-time migration: if the file parses as the old `Vec<String>`, wrap
  each as a `Text` entry. Bump an internal version field.
- **Images:** in the pasteboard watcher (`poll_clipboard`), when no string is
  present, check for image data (`NSPasteboard` PNG/TIFF types via existing
  `NSPasteboard` bindings); write the PNG to `support_dir/clip-images/<hash>.png`
  and store the path. Cap total image storage (`max_image_mb`) and prune oldest
  unpinned images. Pins are exempt from the `cap` eviction.
- **Links:** detect `^https?://` in text entries → mark `Link`.
- Fuzzy search already exists; just include unpinned + pinned, pinned boosted.

**(d) Permissions.** None (pasteboard + own support dir).

**(e) Tradeoffs/risks.** Image storage growth — enforce a byte cap and prune.
Pinned-vs-cap interaction needs care (pins must survive eviction). The format
migration must be lossless and defensive (fall back to empty on parse failure,
as today). Watcher stays O(1) per tick (still gated on `changeCount`).

**(f) Complexity: M.**

---

## 12. Smarter ranking: frecency, aliases, custom hotkeys (#9) — **M**

**(a) UX.** Frequently/recently launched items drift to the top automatically.
`[aliases]` let users map a short token to a command/app (`ss` → "System
Settings"). `[[hotkeys]]` bind a global key to a specific item `id` (launch
without opening the panel).

**(b) Model mapping.** Uses the cross-cutting `Item.id` + `Engine` frecency boost
(§3b/§3c). Aliases: a small map consulted by providers (or by `Engine` as a
query-rewrite: if the query exactly equals an alias, also try the aliased term).
Custom hotkeys: register additional `global-hotkey` entries at startup, each
carrying an `Action` to execute directly (bypassing the panel).

**(c) Implementation.**
- **Frecency store:** `usage.json` = `{ id: { count, last_ts } }` in the support
  dir. `boost(id)` = a bounded function of `count` and recency decay (e.g.
  `min(400, count_weight + recency_weight)`), folded in `Engine::query` (§3c).
  Record on activation in `activate_selection`.
- **Aliases:** load from `[aliases]`; cheapest implementation is an
  `Engine`-level rewrite (if `query == alias`, run providers on the target too
  and merge). Keeps providers unaware of aliases.
- **Custom hotkeys:** parse `[[hotkeys]]`, register via the existing
  `GlobalHotKeyManager`, map each hotkey id → `Action`; in the hotkey listener,
  dispatch the action on the main thread (reuse the `performSelectorOnMainThread`
  pattern) instead of toggling the panel.

**(d) Permissions.** None.

**(e) Tradeoffs/risks.** Frecency must not overpower intentful queries — keep
the boost bounded (calc/keyword scores stay dominant). Persisting usage on every
launch is a tiny JSON write (debounce or write-through is fine). Hotkey
conflicts with system/other apps — surface registration failures (the code
already logs hotkey registration results). Tune decay so stale favorites fade.

**(f) Complexity: M.**

---

## 13. Process manager (#10) — **M**

**(a) UX.** Keyword `kill` (alias `ps`): `kill` lists running apps/processes by
CPU/memory; `kill safari` fuzzy-filters. Enter sends SIGTERM after a
confirmation (`Action::Confirm`); a modifier could force SIGKILL. Show PID, name,
%CPU, %MEM in the row.

**(b) Model mapping.** `ProcessProvider`, keyword-gated. Each process → `Item`
with `Action::Confirm { inner: RunShell("kill <pid>") }`, `source = "Proc"`.
Reuse `fuzzy_score` over process names. No new core types beyond `Confirm`
(already added in §3a).

**(c) Implementation.**
- **List GUI apps:** `NSWorkspace.runningApplications` (already linked) gives
  name + PID + icon (reuse `icon` field) for user-facing apps — the safest,
  most relevant set.
- **List all processes:** `ps -axo pid,comm,%cpu,%mem` parsed into rows (for
  power users). Only run on keyword match (never per keystroke).
- **Kill:** `Action::RunShell("kill <pid>")` (SIGTERM) wrapped in `Confirm`;
  force = `kill -9`. Killing only the current user's processes needs no
  privileges.

**(d) Permissions.** None for own-user processes. (Killing other users' / system
processes would need elevation — explicitly out of scope.)

**(e) Tradeoffs/risks.** Safety is the main concern — **always confirm** before
killing, and consider excluding critical system processes (e.g. `WindowServer`,
`loginwindow`) or warning loudly. Prefer the `NSWorkspace` app list as the
default view (less footgun than raw `ps`). `ps` parsing is cheap and gated by the
keyword.

**(f) Complexity: M.**

---

## 14. Summary of shared work (do these first)

1. `Action`: add `Paste`, `CopyFile`, `Confirm`, `Window`, `AskAiFollowup`.
2. `Item`: add `id: Option<String>` + `with_id` (icon already in progress).
3. `Engine`: add frecency boost + `usage.json` store; record on activate.
4. Config: add `[snippets]`, `[[quicklinks]]`, `[conversion]`, `[window]`,
   `[ai].commands`/`chat`, `[aliases]`, `[[hotkeys]]`, `[clipboard]` — all
   `serde(default)`.
5. UI: extend `activate_selection` to special-case `Confirm`, `Window`,
   `AskAiFollowup` (mirroring today's `AskAi` handling).

These five land before feature providers so each provider is a small, additive
file in `src/providers/` (matching the current pattern).
