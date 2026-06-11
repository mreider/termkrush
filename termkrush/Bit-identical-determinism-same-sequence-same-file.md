---
title: 'Bit-identical determinism: same sequence, same file'
type: feature
created: "2026-06-11T13:33:20Z"
modified: "2026-06-11T19:54:32Z"
author: Matt Reider
status: delivered
estimate: "3"
project: termkrush
started: "2026-06-11T19:48:48Z"
delivered: "2026-06-11T19:54:32Z"
---

## Goal

Make the inception's strict-determinism constraint provable: the same sequence renders the same WAV, **byte for byte**, across runs and across macOS/Linux/Windows. The sequence file plus the library *is* the mix; there is nothing else.

## Engine spec

- Audit the full render path for nondeterminism: unordered map iteration, thread-pool reduction order, platform-varying float paths (fast-math, FMA differences), uninitialized padding in the WAV writer.
- All randomness flows from one seed derived from the input (track content + order + beat marks) — verified by construction (no other entropy source compiles into the render path).

## Acceptance

- Local test: render the golden fixture sequence twice → identical SHA-256.
- CI: the golden-mix hash test runs on macOS, Linux, and Windows and all three produce the same hash (per the working agreement, the cross-OS matrix already exists).
- Mutation check in tests: changing one beat mark or swapping two entries changes the hash.

## Comments

## Attachments
