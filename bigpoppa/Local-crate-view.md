---
title: Local crate view
type: feature
created: "2026-06-04T09:11:01Z"
modified: "2026-06-04T09:11:01Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: one-deck
tags: [library, tui, one-deck]
project: bigpoppa
---

## Problem statement

Hard-coded file paths are not a real load workflow. Need a browsable local crate.

## Possible solution

- Default crate root: `~/Music/bigpoppa` (configurable in `~/.config/bigpoppa/config.toml`).
- Recursive scan for `*.mp3` on startup.
- TUI panel: scrollable list, fuzzy filter on `/`, `enter` loads into the focused deck.
- Show duration and (if cached) BPM next to each filename.

## Acceptance

- [ ] Crate panel lists all mp3s under the configured root.
- [ ] `/` opens a filter; typing narrows the list; `Esc` clears.
- [ ] `enter` loads the highlighted track into the focused deck.
- [ ] Config file path and crate root are documented in README.
