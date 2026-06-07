# Inception

> **Pivot (2026-06-07).** The original two-deck turntable model was wrong. Based
> on user feedback we are going back: there are **no decks**. Everything is a
> **pad**. The tool is a pad-based, timeline-arranged scratch/loop mixer whose
> job is to keep everything in tempo automatically so anyone can make a good
> old-school-scratch mix. The deck-era stories below the current accepted work
> are obsolete and being retired.

## The user

Any sort of idiot — someone who wants to make an awesome mix with old-school
scratching but has no DJ skills and no turntables. They keep mp3s in folders,
they're comfortable in a terminal, and they want the software to handle timing
so nothing is ever out of beat. Keyboard- and Xbox-first; one binary; CRT feel.

## The goal

When we ship, this user can:

1. Build a **track list** — folders of audio (WAV native; import/export MP3),
   managed on the filesystem (drop files in the directory; rename/delete/move
   in the UI).
2. Load tracks onto **pads**. A pad is one of three kinds:
   - **Loop** — a track (or trimmed region) that repeats, auto-synced to the
     master tempo so its beats always land on the grid.
   - **Scratch** — a very short clip; the software finds the scratch point
     (onset) and you build **whip/wiki** phrases over it.
   - **One-shot** — plays its clip once, normally.
3. Rely on **automatic BPM sync**: the first loop sets the master tempo; every
   other loop **varispeeds** to it (pitch rides, like a real platter). Speed the
   whole mix up or down together; beats stay locked across all loops.
4. **Scratch** the old-school way: *whip* = backward rub with the forward motion
   muted ("whip whip whip"); *wiki* = forward rub that sounds; combine into
   "wiki-whip / whip-wiki" and longer phrases. You tap a rhythm of whips/wikis
   and it's recorded as the pad's scratch phrase, quantized to tempo.
5. Control **per-pad volume**, and **activate/deactivate** pads with a hard cut
   or a soft fade. Pads stack freely — no crossfader needed.
6. **Arrange** pads on a tempo-locked **step-grid timeline** (tracker-style,
   lanes per pad, always quantized). Loops repeat to fill the span you draw.
7. **Render** the arrangement to a track (WAV; export MP3), saved into the list
   — over an existing track or as a new one. Reload any saved track onto a pad
   to trim it down, re-tempo, adjust volume, and save back.

No decks, no crossfader, no requirement to perform live. The point is *making*
great old-school-scratch mixes where timing is automatic.

## The reason

DJ software is heavy, GUI-bound, and skill-gated; a good scratch mix normally
needs turntable chops. There is no terminal-native, keyboard-first tool that
makes **old-school scratching and tight, auto-synced loops accessible to anyone**
by modeling the scratch sounds directly and taking timing off the user's plate.
That's the gap, in a binary small enough for one person to maintain.

## Success

A non-DJ runs `termkrush`, builds a short track list, drops a couple of loops
that snap to one tempo automatically, lays a few whip/wiki scratches on the
grid, and renders a mix that actually sounds good — without reading docs. The
first release marker is the moment that becomes true.

Beyond that: GitHub stars/forks signal interest, buymeacoffee signals
appreciation, and people posting rendered mixes signals real use.

## Constraints

- **Single binary.** No required runtime deps beyond the OS audio stack. An MP3
  encoder is bundled (export is a first-class feature now); no external tools.
- **WAV native, MP3 import + export.** Tracks live as files in folders.
- **Filesystem-managed library.** Folders are directories; files move in/out of
  the directory normally — nothing special in the UI, no in-app downloads.
- **Everything stays in tempo automatically.** This is the core promise; loop
  sync is non-optional and zero-config (first loop sets it).
- **Cross-platform**, **MIT**, **solo-maintainable**, keyboard + Xbox,
  CRT amber/green identity echoed in the TUI.

## Out of scope

- Decks, a crossfader, or turntable/deck emulation as the interaction model —
  replaced wholesale by pads.
- In-app URL / yt-dlp downloads — the library is filesystem-managed.
- Pitch-preserving sync — we chose varispeed (platter feel). The offline
  time-stretch engine stays in the tree as a possible future "warp" option but
  does not drive loop sync.
- GUI / web / mobile; streaming services; stems / vocal isolation; networked or
  cloud-synced sessions.

## Note on the prior build

The accepted work (clip engine, sampler voice, trim, BPM detect/cache, library
scan, focus→act grid, Xbox input) largely survives and gets repurposed. The
**deck module, crossfader/auto-fade deck-blend, deck sync, deck cue, and
deck-centric controls are removed**. The old playback pattern set (cut /
transformer / stutter / warble / reverse) is dropped in favor of the real
whip/wiki scratch model. The release definition changes: **v0.1.0 is now the
pad + loop-sync + scratch + timeline + render MVP**, not the two-deck mix.
