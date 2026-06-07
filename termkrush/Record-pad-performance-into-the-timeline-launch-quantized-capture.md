---
title: Record pad performance into the timeline (launch-quantized capture)
type: feature
created: "2026-06-07T17:52:36Z"
modified: "2026-06-07T17:54:05Z"
author: Matt Reider
status: unstarted
estimate: "8"
epic: looper
project: termkrush
---

## Goal
Build the arrangement by performing: arm record, trigger pads, each captured onto the timeline starting on the next bar (launch-quantized).

## Spec
- An arm-record toggle on the timeline.
- While armed + transport running, a pad trigger writes a block onto that pad's lane starting at the next bar boundary (via the quant engine).
- Stop record; captured blocks remain and are editable. This becomes the primary arrange path (replaces cell-toggle).
