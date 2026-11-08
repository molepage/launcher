# Launcher

A small, fast macOS launcher: Rust core, thin Swift/AppKit shell. About
5 MB on disk and ~30 MB of memory with the file index loaded. Uses no CPU
while idle.

## Build and run

```bash
scripts/build.sh            # → build/Launcher.app
scripts/build.sh --install  # also copies to ~/Applications and (re)starts it
```

Needs Rust and Xcode's command-line tools. The build signs with your Apple
Development certificate when you have one. That keeps the Accessibility
permission across rebuilds; ad-hoc builds lose it each time.

## Using it

| Do | Get |
|---|---|
| **⌥Space** | open / close (change `hotkey` in config once Raycast is gone) |
| type | apps, quicklinks, commands, add-ons; maths like `12*3` |
| `f <name>` | files and folders in your home folder and iCloud Drive |
| `ss <name>` | screenshots (newest first, with thumbnails) |
| `clip <text>` or **⌘⇧V** | clipboard history |
| `g <text>` | Google (any quicklink with `{query}` works this way) |
| `quit <app>`, `sh <cmd>` | the two example add-ons |
| ↑ ↓ / ⌃N ⌃P | move |
| ↩ | open (paste for clips, copy for screenshots and sums) |
| ⌘↩ | show in Finder / copy instead of paste |
| ⌘1…⌘9 | pick a row |
| ⇥ | jump into a command (e.g. "Clipboard History" → `clip `) |
| esc | clear, then close |

Ranking learns from what you pick. If you pick the same thing for the same
text twice in a row, it sticks to the top for that text.

Pasting from clipboard history needs **System Settings → Privacy & Security
→ Accessibility → Launcher**. macOS asks the first time. Without it, the clip
is copied and you press ⌘V yourself.

## Settings

`~/Library/Application Support/Launcher/config.toml` is created on first
run. It holds hotkeys, extra app folders and quicklinks. Type `settings` in
the launcher to open it.

## Changes from the original launcher

**Requirements**: macOS 12 or later.

**Settings window**
- A gear button appears when you hover over the search bar, opening a settings window
- Choose light, dark, or system appearance
- Set or change the global hotkeys for the launcher and clipboard history
- Add extra folders to scan for apps
- Toggle file search on/off and choose which folders it indexes
- Toggle System Settings search: type things like "firewall" to jump straight to the right pane
- Everything saves instantly and applies live; the window scrolls on small screens

**Search bar polish**
- Rounded corners and the panel can be dragged anywhere on screen

**A sleeping cat**
- A pixel-art cat sleeps on top of the search box while you type
- It can be turned off in settings

## Add-ons

An add-on is a folder in `~/Library/Application Support/Launcher/addons/`
with an `addon.toml` (name, keyword, exec). It contains a program that gets
the typed text as its first argument and prints JSON results. See
`addons/quit` for a 20-line example. The full protocol is in
`docs/superpowers/specs/2026-11-04-launcher-design.md`.

## Layout

- `core/`: Rust. Matching (`matcher.rs`), learning (`learn.rs`), file index
  (`fileindex.rs`), clipboard store (`clips.rs`), add-on runner (`addons.rs`,
  `runner.rs`) and the C ABI (`lib.rs`). `cargo test` runs 36 tests.
  `cargo run --release --bin lq -- <query>` tries queries from a terminal.
- `shell-macos/`: Swift. Panel, result drawing, hotkeys, clipboard watcher,
  actions.
- `addons/`: example add-ons, bundled and installed on first run.
- `docs/`: the design record.
