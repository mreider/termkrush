---
title: Import a YouTube song as WAV to the track list
type: feature
created: "2026-06-07T11:13:51Z"
modified: "2026-06-07T11:13:51Z"
author: Matt Reider
status: unstarted
estimate: "5"
---

## Problem
Let the user pull a track in by URL: shell out to `yt-dlp` (optional, never bundled — matches the single-binary constraint), decode the result, and write a **WAV** into the track list. This is the one sanctioned "download" path; the library is otherwise filesystem-managed. Later story (post-v0.1.0).

## Acceptance
- [ ] Paste/enter a YouTube (or media) URL; `yt-dlp` fetches audio if it's on PATH (graceful message if not).
- [ ] The audio is converted to a WAV and dropped into the current library folder, named from the title.
- [ ] It appears in the track list and loads onto a pad like any other track.
- [ ] yt-dlp is shelled out to, never bundled; absence is a soft failure, not a crash.
