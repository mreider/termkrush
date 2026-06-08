---
modified: "2026-06-08T08:33:58Z"
---

## Goal
Serve the project site at https://termkrush.com (currently mreider.github.io/termkrush).

## Steps
- Porkbun DNS for termkrush.com: apex `A` records → GitHub Pages IPs
  (185.199.108.153 / .109.153 / .110.153 / .111.153), `AAAA` to the v6 set, and
  `CNAME www` → mreider.github.io. (Porkbun has an API; do it via curl or their console.)
- GitHub Pages: set the custom domain to `termkrush.com` (adds a CNAME file to
  the Pages source), wait for the cert, then enable "Enforce HTTPS".
- Verify both apex and www resolve + serve over HTTPS.

## Security
- Porkbun API keys were provided out-of-band. Do NOT commit them or put them in
  this story/repo; pass them via env vars at run time only. Rotate after use.
- This is an outward-facing change to a live domain — confirm with the PM before applying.
