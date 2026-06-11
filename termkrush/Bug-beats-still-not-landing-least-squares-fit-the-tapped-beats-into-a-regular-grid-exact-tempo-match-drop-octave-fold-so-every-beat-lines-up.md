---
title: 'Bug: beats still not landing - least-squares fit the tapped beats into a regular grid + exact tempo match (drop octave-fold) so every beat lines up'
type: bug
created: "2026-06-10T19:17:52Z"
modified: "2026-06-10T20:15:53Z"
author: Matt Reider
status: delivered
started: "2026-06-10T19:17:53Z"
finished: "2026-06-10T20:08:54Z"
delivered: "2026-06-10T20:15:53Z"
project: termkrush
---

## Problem statement

## Possible solution

## Comments

## Attachments

## Rejection notes

- 2026-06-10: Loop varispeed ratio inverted at mix/mod.rs:661 — secondary pads play at clip/master instead of master/clip, so any clip whose tempo differs from the master is sped/slowed the wrong way (effective tempo = clip^2/master). Only correct when clip_bpm == master_bpm, which is why the master pad looks fine and every secondary's beats slip. Fix: flip to master/pad to match arrangement.rs:65 (target/clip).
