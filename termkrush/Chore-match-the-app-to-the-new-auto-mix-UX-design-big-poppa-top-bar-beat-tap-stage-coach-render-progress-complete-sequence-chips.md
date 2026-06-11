---
title: 'Chore: match the app to the new auto-mix UX design (big poppa): top bar, beat-tap stage, coach, render progress + complete, sequence chips'
type: chore
created: "2026-06-11T18:53:25Z"
modified: "2026-06-11T19:07:03Z"
author: Matt Reider
status: started
started: "2026-06-11T18:53:25Z"
---

## Goal

Match the egui app to the PM's new auto-mix UX design ("big poppa" package; source versioned at `docs/design/automix/` — seven screens covering the three surfaces plus the states the app lacked).

## What landed

- **Top bar** — brand left; status right: crate path and the master-grid readout ("grid locked · 92 bpm master · track_01" / "no grid"). Zero knobs.
- **Beat-tap stage** — bar-numbered ruler from the fitted grid, region shading outside the trim, fitted beat grid (downbeats stronger + numbered), the raw taps as carets in a ↓-taps lane, play-time/duration readout, and a fit-stats footer: tempo · downbeat · taps fit · residual (±ms RMS of taps vs. grid), with the "● fitting live" / "✓ saved to beats.txt" pill and the ↓ tap-key affordance. Amber while provisional, green once saved.
- **Coach** — first-run central card: "tap a beat, once" + the 1-2-3 steps.
- **Render progress panel** — central: spinning vinyl, percent bar, phase chips (decode → grid fit → arrange + scratch → bounce WAV), the determinism line. Worker now reports phases over the render channel.
- **Render complete panel** — central: file name, length/tempo/phrases/format specs, the seed tag ("same sequence → same mix, bit for bit"), and actions: play mix, reveal in library, **export → MP3** (wired to core's `export_mp3`), dismiss.
- **Sequence line** — status pills (✓ ready / ◷ N need beats / empty / rendering N% with a progress strip), "autosaved · sequence.txt" note, master mini-readout, amber "▶ render mix" button with a disabled state; chips with order number, ✕, tempo or "needs beats ✎", "sets tempo" on the first entry, and a path-seeded pseudo-waveform mini (the design's waves are seeded fakes too); › swap separators and a trailing ＋ drop chip.

## Acceptance

- `cargo test` green (115 tests), fmt + clippy clean.
- Visual check against `docs/design/automix/main.png` and the screen states in `tk-screens.jsx` via `scripts/dev-run.sh gui`.

## Comments

## Attachments
