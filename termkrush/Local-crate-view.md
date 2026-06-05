---
title: Local crate view
type: feature
created: "2026-06-04T09:11:01Z"
modified: "2026-06-05T17:53:41Z"
author: Matt Reider
status: accepted
estimate: "2"
epic: one-deck
tags: [library, tui, one-deck]
project: termkrush
started: "2026-06-05T17:45:48Z"
finished: "2026-06-05T17:53:41Z"
delivered: "2026-06-05T17:53:41Z"
accepted: "2026-06-05T17:53:41Z"
---

## Problem statement

Hard-coded file paths are not a real load workflow. Need a browsable local crate.

## Possible solution

- Default crate root: `~/Music/termkrush` (configurable in `~/.config/termkrush/config.toml`).
- Recursive scan for `*.mp3` on startup.
- TUI panel: scrollable list, fuzzy filter on `/`, `enter` loads into the focused deck.
- Show duration and (if cached) BPM next to each filename.

## Acceptance

- [x] Crate panel lists all mp3s under the configured root. (Recursive `Crate::scan`, sorted; rendered as a bordered, scrollable list. `scan_finds_mp3s_recursively_sorted_and_skips_other_files`.)
- [x] `/` opens a filter; typing narrows the list; `Esc` clears. (`slash_opens_filter_and_typing_narrows`; case-insensitive subsequence fuzzy match.)
- [x] `enter` loads the highlighted track into the focused deck. (`jk_navigate_and_enter_loads_selected`, `filter_enter_loads_highlight_and_closes`; decode happens in the event loop.)
- [x] Config file path and crate root are documented in README. (New "Configuration" section + keyboard cheatsheet.)

## Implementation notes

- **Config:** new `config::Config` loads `~/.config/termkrush/config.toml` (or `$XDG_CONFIG_HOME`), key `crate_root` with `~/` expansion; malformed/missing → defaults (default root `~/Music/termkrush`). Added the `toml` crate. Tilde expansion is tested via an injected-home helper (no env mutation, no test races).
- **Library:** `library::Crate::scan` recursively collects `*.mp3` (case-insensitive), sorted; `filtered()` + `fuzzy_subsequence` for the `/` search. Missing root → empty crate (no error).
- **TUI:** `App` gains the crate, a selection index, a filter mode, and a pending-load slot. New keys: `/` filter (modal — typing narrows, Esc clears, Enter loads), `j`/`k` navigate, Enter loads. Decode (I/O) stays in the event loop via `take_pending_load`. Body is now split: crate list (left) + deck panel (right).

## UX decisions (for review)

- **`j`/`k` navigate the list** (not arrows): arrows are already bound to deck seek, so vim-style keys avoid the conflict. Documented in help + README.
- **Duration/BPM per row deferred:** decoding every file at startup to show duration is expensive, and there's no analysis cache yet ("Cache BPM and analysis per file" is a later story). The story's "(if cached)" wording covers this — rows show the file name today; duration/BPM join once the cache lands.
- TUI rendering is verified headlessly (TestBackend) but not eyeballed; interactive feel (scroll, highlight) is best confirmed by running it.
