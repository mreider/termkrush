---
title: Direct URL mp3 download
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: download
tags: [download, net]
project: termkrush
---

## Problem statement

Some sources serve mp3 directly. We should be able to paste a URL and get the file.

## Possible solution

- reqwest blocking client on a background thread; stream to disk.
- Save to ~/Music/termkrush/_downloads/, named from the URL's basename (sanitized).
- Progress reported back to the TUI panel via a channel.
- Resume on partial download (HTTP Range) if server supports it.

## Acceptance

- [ ] Pasting a direct .mp3 URL downloads it to the crate.
- [ ] Progress bar reflects bytes received.
- [ ] Cancelling mid-download deletes the partial file.
- [ ] Errors surface in the panel without crashing.
