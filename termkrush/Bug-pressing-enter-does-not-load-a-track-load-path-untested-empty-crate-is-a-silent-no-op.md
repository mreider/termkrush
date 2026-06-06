---
title: 'Bug: pressing enter does not load a track (load path untested, empty crate is a silent no-op)'
type: bug
created: "2026-06-06T09:06:02Z"
modified: "2026-06-06T09:06:36Z"
author: Matt Reider
status: unstarted
project: termkrush
---

## Symptom

Pressing `enter` does nothing — no track loads onto the deck. Tests were green, so the failure was invisible.

## Cause

Two compounding issues: (1) `crate_root` defaulted to `~/Music/termkrush`, which doesn't exist, so the crate was empty (0 tracks) and `enter` had nothing to select → silent `Action::None`; (2) the actual load path (event loop: action → `decode_file` → `deck.load`) was **never integration-tested**, so a broken or no-op load couldn't be caught.

## Fix

- Make the event-loop load step testable (lift it out of `event_loop`) and assert `enter` decodes + loads the selected track onto the focused deck end-to-end against a real fixture.
- Empty-crate state gives clear guidance (done in the truncation bug) so `enter`-does-nothing is never mysterious.

## Verification

- [ ] An end-to-end test: populated crate → `enter` → focused deck holds the decoded track.
- [ ] Loading works in the running app with `crate_root` configured.
- [ ] cargo test green.

## Related

Defect against Local crate view + the ergonomic-keymap load wiring. Pairs with the integration-tests chore (built first).
