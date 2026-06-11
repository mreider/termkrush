---
title: Beat-tap any library track, cached for good
type: feature
created: "2026-06-11T13:32:46Z"
modified: "2026-06-11T13:37:17Z"
author: Matt Reider
status: unstarted
estimate: "3"
project: termkrush
---

## Goal

Beat marks are the engine's only required input besides track order, and a track is tapped **once, ever**. Promote the existing tap flow (play, tap on each beat, least-squares grid fit → exact tempo + downbeat) from the pad-era clip editor to a first-class library action.

## User-visible change

- Tap beats on any track straight from the library (and from a sequence entry's "needs beats" badge).
- Marks (fitted tempo + downbeat anchor) cache per track and survive app restarts and library renames/moves.
- A track with marks shows its tempo in the library list; re-tapping replaces the marks.

## Acceptance

- The least-squares fit path is reused, not duplicated; existing fit tests keep passing.
- Cache round-trip test: tap → restart → marks present; rename/move → marks still attached.
- A sequence whose entries all have marks reports "ready to render"; one without reports which entries need tapping.

## Comments

## Attachments
