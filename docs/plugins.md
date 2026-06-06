# litecast plugins

litecast can be extended with external plugins: any executable placed in

```
~/Library/Application Support/litecast/plugins/
```

## How plugins are triggered

A plugin's **keyword** is its file name without extension. For example an
executable named `weather` (or `weather.sh`) has the keyword `weather`.

A plugin is invoked only when the **first word** of the query matches its
keyword. This keeps litecast fast: it never spawns plugin processes unless you
explicitly type their keyword. The rest of the query is passed to the plugin as
its single command-line argument.

Example: typing

```
weather Berlin
```

runs `~/Library/Application Support/litecast/plugins/weather` with `argv[1] = "Berlin"`.

## Output contract

The plugin must print a single JSON document to **stdout**:

```json
{
  "items": [
    {
      "title": "Berlin: 18C, partly cloudy",
      "subtitle": "Press Enter to open forecast",
      "action": "open",
      "target": "https://wttr.in/Berlin"
    }
  ]
}
```

Fields per item:

- `title` (required): main text shown in the result row.
- `subtitle` (optional): secondary text.
- `action` (optional, default `open`): one of
  - `open` - open `target` as a file, folder, app, or URL (`/usr/bin/open`).
  - `shell` - run `target` with `/bin/sh -c`. Requires a second Enter to confirm.
  - `copy` - copy `target` to the clipboard.
  - `none` - informational only.
- `target` (optional): argument for the action.

## Rules and limits

- The plugin must be executable (`chmod +x`).
- Output must be valid JSON; anything else is ignored.
- Plugins have an 800ms timeout, after which they are killed.
- Plugins run on a background thread, so they never block typing.

## Minimal example

`~/Library/Application Support/litecast/plugins/echo`:

```sh
#!/bin/sh
# Usage: echo <text>
printf '{"items":[{"title":"You typed: %s","action":"copy","target":"%s"}]}' "$1" "$1"
```

Then `chmod +x echo` and type `echo hello` in litecast.
