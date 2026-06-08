---
modified: "2026-06-08T10:17:06Z"
---

## Goal
Space on a highlighted library song plays a quick preview (so you can audition before loading).

## Why it's its own story
Preview requires decoding the file (it's not on a pad yet). Needs the async decode path extended: a LoadTarget::Preview (or similar), a pending-preview, and a mixer one-shot "preview voice" at unity gain that place_decoded triggers on completion. Keep it from stacking (stop any prior preview first), consistent with the no-stack fix.
