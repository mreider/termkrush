---
title: 'Bug: crate panel text truncated (empty-state message and long track names cut off at panel width)'
type: bug
created: "2026-06-06T08:53:29Z"
modified: "2026-06-06T08:56:41Z"
author: Matt Reider
status: finished
started: "2026-06-06T08:53:39Z"
finished: "2026-06-06T08:56:41Z"
project: termkrush
---

## Symptom

In the crate panel the empty-state message and long track names are cut off at the panel's right edge (e.g. `(no mp3s — set crate_root, s` with "ee README" missing).

## Cause

The crate panel is a fixed 32-col column. The empty-state hint was a single long `ListItem` (~40 chars) and track names were rendered raw — both hard-truncated by the List widget with no wrapping or ellipsis.

## Fix

- Empty crate (not filtering) now renders a **wrapped** `Paragraph` how-to that fits the panel width (phrased so no token splits at 30 cols).
- Track names are **ellipsized** (`name…`) to the inner width via a new `ellipsize` helper, so long titles end cleanly instead of being chopped.

## Verification

- [x] `empty_crate_shows_wrapped_help_not_truncated` — `crate_root` + `config.toml` survive in full.
- [x] `long_track_names_are_ellipsized_in_the_crate` — overflowing name shows `…`.
- [x] `ellipsize_truncates_long_with_marker` unit test.
- fmt / clippy -Dwarnings / full suite (93 lib) green.

## Related

Defect against the accepted **Local crate view**. (Separately: the crate was empty because `crate_root` defaulted to `~/Music/termkrush`; configuring it to the user's folder is a config change, not part of this fix.)
