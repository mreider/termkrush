---
title: Awesome UX revamp functional great-looking turntable interface
type: feature
created: "2026-06-06T09:06:02Z"
modified: "2026-06-06T14:18:34Z"
author: Matt Reider
status: accepted
estimate: "8"
epic: ux
project: termkrush
started: "2026-06-06T09:12:40Z"
delivered: "2026-06-06T09:18:07Z"
accepted: "2026-06-06T14:18:34Z"
---

## Problem statement

The interface works but isn't yet *awesome* — a deliberate visual + interaction pass. Open questions: is the crossfader too wide / does it feel good? proportions? focus clarity? cropped text? does it look like gear you'd want to perform on?

## Acceptance

- [x] Crossfader is right-sized (not full-width), clearly A↔B with a center mark, and reads as a real fader. (Fixed-width ~25 centered throw with `┼` center detent + `●` handle; was console-wide.)
- [x] No truncated/cramped text. (Deck track names now ellipsized like the crate list; empty crate shows wrapped guidance.)
- [x] First-run/empty state clearly says how to get tracks playing. ("No tracks found. Set crate_root in your config.toml — see the README.")
- [x] Every control still works — guarded by the new end-to-end integration tests.
- [ ] Side-by-side decks read as turntables and are the visual focal point (bigger/better platters, tonearm/label). — **needs visual review**
- [ ] Demonstrably looks awesome (screenshot review). — **the PM's call; I render headlessly**

## Status: first concrete pass, delivered for review

Done blind-safe (objective) improvements:
- Crossfader right-sized + center detent (your "is it too wide?" — yes, fixed).
- Deck + crate text no longer truncates; clear empty state.
- All commands covered end-to-end so the revamp can't silently break controls.

What's left is **subjective/visual** (bigger turntables, proportions, color hierarchy, "awesome") and is hard to judge without a real terminal. Recommend: accept this functional pass and **decompose** the visual revamp into sub-stories you steer with screenshots — I can't honestly self-certify "looks awesome."

A text render at 100x30 (decks A/B with platters + BPM, right-sized crossfader, wrapped empty crate) is in the chat for reference.
