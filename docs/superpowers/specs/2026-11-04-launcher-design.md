# Launcher — design record (v0.1)

Written 2026-11-04. The user chose to
skip further design questions and go straight to building, so this records
what was agreed in conversation plus the calls made while building.

## Intent

A personal macOS launcher that replaces Raycast. It must be very fast and
light, and do only what the user needs. No accounts, cloud features or
freemium upsell. A Windows version may follow, so the logic is portable.

## Agreed

- **Rust core plus a thin native shell per OS.** On macOS the shell is
  Swift/AppKit. The core holds all logic: indexing, matching, ranking,
  learning, storage and add-ons.
- **Built-in (core) features:** app search, file search, clipboard history,
  screenshot browsing and quicklinks.
- **Everything else is an add-on.** Add-ons run as separate processes and
  print JSON. A slow or broken add-on can never slow down or crash search.
- **Ranking learns from use**: picks for the exact query beat
  general recent use, which beats raw match quality. Repeating a pick for the
  same query twice "latches" that item to the top.

## Calls made while building (check these)

- **Hotkey defaults to ⌥Space** because Raycast currently holds ⌘Space.
  Change it in `config.toml` once Raycast is gone.
- **Clipboard history hotkey defaults to ⌘⇧V.** Also opened by typing `clip `.
- **Bridge is a small hand-written C ABI passing JSON**, not UniFFI. There are
  only about eight calls and a few dozen results per keystroke. That keeps
  the build simple and dependency-free, and works the same for a Windows
  shell later.
- **Keywords:** `f ` searches files (home folder and iCloud Drive),
  `ss ` lists screenshots, `clip ` shows clipboard history. Quicklinks and
  add-ons declare their own keywords. App search and quicklinks show with no
  keyword. A maths expression shows an inline answer.
- **File search uses our own in-memory name index, not Spotlight.**
  Spotlight (`mdfind`) took about 2 s per query on the target machine. The
  index walks home and iCloud Drive in the background (~340k entries, a few
  seconds, ~25 bytes each) and rebuilds when it's older than 15 minutes.
  A search is a parallel fuzzy pass taking 3–30 ms. Live FSEvents updates and
  content search are possible later upgrades.
- **Clipboard history is text only in v0.1.** It keeps 1,000 entries for up to
  90 days. Anything a password manager marks as concealed or transient is
  never stored.
- **Pasting** puts the item on the clipboard and sends ⌘V to the previous app.
  This needs Accessibility permission. Without it, the item is only copied.
- **Calculator** is a tiny built-in expression parser (+ − × ÷ ^ %,
  brackets). There is no currency conversion yet.
- **The window** is one borderless, non-activating panel. It is created once
  and hidden when it loses focus. Results are drawn in a single custom view.
  It shows only the search bar until there are results (compact mode).

## Add-on protocol (v1)

An add-on is a folder in `~/Library/Application Support/Launcher/addons/`
holding an `addon.toml`:

```toml
name = "Quit Applications"
keyword = "quit"          # typed as "quit <query>"
exec = "run.sh"           # relative to the folder; gets the query as argv[1]
timeout_ms = 1000         # optional
```

The program prints a JSON object on stdout:

```json
{"items": [{"uid": "stable-id", "title": "Safari", "subtitle": "Quit",
            "icon": {"kind": "file", "value": "/Applications/Safari.app"},
            "action": {"type": "run", "argv": ["osascript", "-e", "quit app \"Safari\""]}}]}
```

- `uid` is required and must be stable; learning keys on it.
- Action types: `open` (path), `open_url` (url), `copy` (text), `paste`
  (text), `reveal` (path), `run` (argv).
- When a new keystroke arrives, the core kills the previous run before
  starting the next one. A run that goes past its timeout is killed and
  shows nothing.

## Non-goals for v0.1

- Settings UI (edit `config.toml`)
- Image clipboard history
- Own file index
- A Windows shell
- An add-on store
- AI chat
- Focus sessions
