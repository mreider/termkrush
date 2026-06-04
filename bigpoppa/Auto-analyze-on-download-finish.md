---
title: Auto-analyze on download finish
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: download
tags: [download, tempo]
project: bigpoppa
---

## Problem statement

Fresh downloads should be ready to mix immediately — BPM already known.

## Possible solution

- Hook the download-complete event to enqueue a BPM analysis task on the new file.
- Crate row shows analyzing until done, then BPM.

## Acceptance

- [ ] Finishing a download triggers analysis without user action.
- [ ] Crate row reflects analysis state and final BPM.
- [ ] No UI freeze during analysis.
