---
title: GitHub Pages from /docs
type: feature
created: "2026-06-04T09:15:06Z"
modified: "2026-06-04T09:15:06Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: release-v0-1
tags: [site, release, docs]
project: termkrush
---

## Problem statement

The landing page mock is at /index.html but Pages is not configured.

## Possible solution

- Move index.html into docs/index.html, adapt:
  - Replace you/termkrush placeholder with mreider/termkrush everywhere.
  - Wire the download CTA to releases/latest.
  - Add buymeacoffee button (small inline icon, link to buymeacoffee.com/mreider).
- Configure repo: Pages -> main / /docs folder.
- Add a tiny favicon (amber vinyl).

## Acceptance

- [ ] https://mreider.github.io/termkrush/ renders the landing page with CRT scanlines.
- [ ] Download button leads to releases/latest.
- [ ] BMAC link opens buymeacoffee.com/mreider.
- [ ] All links resolve (no 404 to placeholder repo).
