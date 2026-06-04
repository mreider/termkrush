---
title: Yt-dlp shell-out for media URLs
type: feature
created: "2026-06-04T09:20:27Z"
modified: "2026-06-04T09:20:27Z"
author: Matt Reider
status: unstarted
estimate: "3"
epic: download
tags: [download, yt-dlp]
project: termkrush
---

## Problem statement

Most music links the user has are YouTube / SoundCloud / Bandcamp. yt-dlp handles them; we delegate.

## Possible solution

- Detect yt-dlp on PATH at startup; if missing, show an install hint.
- Spawn yt-dlp -x --audio-format mp3 --audio-quality 0 with an output template into the crate dir.
- Capture stdout/stderr; parse the progress lines.
- Show progress in the same download panel as direct URLs.

## Acceptance

- [ ] With yt-dlp installed, pasting a YouTube URL downloads and converts to mp3 in the crate.
- [ ] With yt-dlp missing, the panel shows a clear install hint and does not crash.
- [ ] Progress reflects yt-dlp's reported percentage.
- [ ] The resulting mp3 is decodable by the existing pipeline.
