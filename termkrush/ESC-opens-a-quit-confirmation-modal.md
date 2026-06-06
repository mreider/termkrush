---
title: ESC opens a quit confirmation modal
type: feature
created: "2026-06-06T14:19:26Z"
modified: "2026-06-06T14:20:49Z"
author: Matt Reider
status: unstarted
estimate: "2"
epic: ux
project: termkrush
---

## Intent
ESC opens a centered `Quit TermKrush? (y/n)` modal instead of bare `q` quitting instantly — no fat-finger exits mid-set.
## Acceptance
- [ ] ESC opens a Quit? (y/n) modal; `y` quits, `n`/ESC cancels.
- [ ] `Ctrl-C` still hard-quits; bare `q` retired (or opens the same modal).
- [ ] Modal renders centered over a dimmed UI; covered by on_key + draw tests.
