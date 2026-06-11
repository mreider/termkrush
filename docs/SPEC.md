# TermKrush — Product Spec

*A single-binary, deterministic **auto-mixer** for people who can't DJ. Drop
tracks onto a sequence line in the order you want, tap each track's beat once,
hit render — and the engine produces a continuous old-school-scratch mix:
one tempo, seamless phrase swaps, engine-placed scratches and bass drops,
dynamics that breathe.*

This spec describes the product as it stands today and what remains. It
reflects four pivots recorded in `.am/inception.md`:

1. **No decks — everything is a pad** (2026-06-07).
2. **Looper timeline** — perform the arrangement, don't hand-toggle a grid
   (2026-06-07).
3. **GUI pivot** — native **egui desktop app**, mouse-first (2026-06-08).
   *Still stands.*
4. **Auto-mix pivot** (2026-06-11) — pads, the timeline, and all performance
   surfaces are retired. **The user curates; the engine executes the craft.**

Status legend: **✅ done** (built; delivered/awaiting PM acceptance) ·
**🔜 backlog** (specced, not built) · **🗑 retired** (built once, superseded).

---

## 1. What it is

A non-DJ keeps audio files in folders. They order tracks on a **sequence
line** (the same track may appear at positions 1, 3, and 5), **tap beats**
once per track, and hit **render**. A deterministic **mix grammar engine**
does everything a skilled DJ would: locks one master tempo, picks phrase
sections, swaps them seamlessly, scratches, drops the bass, and shapes the
energy arc. Same input → bit-identical mix, every time. **Zero knobs.**

One binary, cross-platform (Win/Mac/Linux), MIT, CRT amber/green identity.

**Run it:** `scripts/dev-run.sh gui` (auto-builds only when the binary is
missing — after a code change run `scripts/dev-run.sh build` first).

---

## 2. The mix grammar

The engine's rules are **measured, not invented**: they come from a
quantitative analysis of a reference hour-long professional mix
(2026-06-11). These numbers are the spec for the engine stories.

| Rule | Measured basis |
|---|---|
| **One master tempo.** First track in the sequence sets it; every section varispeeds to it (pitch rides, platter-style). Half-time feel allowed; tempo changes are not. | The whole reference hour sits on a single 103.4 BPM grid (beat std-dev 20 ms); "faster" windows were half-time feel, not tempo moves. |
| **Phrase sections.** Each sequence entry contributes 8–16 phrase-aligned bars (occasionally up to 32). Repeat entries get different material from the same track. | 91 sections, one per ~40 s; lengths cluster at 8–16 bars (median ~15); boundaries land on downbeat/phrase positions well above chance. |
| **Equal-loudness swaps.** Tracks loudness-normalize at analysis time; the default transition is a swap on a phrase boundary at matched loudness. ~¼ of boundaries are hard cuts (punctuation); fades are rare (~1 in 20). | Median loudness step across 91 transitions: 0.0 dB; 26 hard cuts; 4 ramped fades. |
| **Macro quantized, micro human.** Scratch flurries (1–2 s, whip/wiki, built from the sequence's own tracks) and ~50 ms fader chops start on the grid but keep **un-quantized** internal timing (seeded jitter — loose but reproducible). Density is clustered, leaning on beat 2. | ~1,700 micro-cuts at chance-level 16th-note alignment; ten 1–2 s scratch flurries clustered in one stretch, starting near beat 2. |
| **Bass drops.** ~16 per rendered hour: the low band ducks >10 dB for 1–16 s, bar-quantized, back on the one. | 16 low-band dropout events of 1–16 s in the reference. |
| **Energy waves.** The loudness arc oscillates (~6–8 min period, ~0.4–0.7 of peak) — never a monotonic ramp; spectral balance warms over the back half. | Measured arc oscillation and rising low/high ratio in the reference. |

**Determinism is a constraint, not a feature:** all randomness derives from a
seed computed from the input (track content + order + beat marks). No wall
clock, no unseeded entropy, no platform-varying float paths in the render
pipeline. The project file plus the library *is* the mix.

---

## 3. Architecture

- **`termkrush-core`** — the headless engine, zero UI dependencies: audio
  decode/encode/resample, cpal output, the mixer + voices, varispeed, the
  beat-grid least-squares fit, the **sequence** (project file), the
  **beat-mark cache**, the library (filesystem), config. The mix grammar
  engine lands here as its stories are built. Unit-tested in isolation.
- **`src/gui`** — the **egui/eframe** desktop front-end. View + input only;
  audio is pumped to the cpal ring each frame.

---

## 4. UX & interaction design

Three surfaces, no modes, no modals; CRT amber/green on near-black,
monospace, matching the site.

- **Library (left).** The filesystem-managed crate: browse folders, preview
  (▶), rename (double-click), move (drag to folder, spring-loaded), delete
  (trash), pencil opens the beat-tap editor. Tapped tracks wear their fitted
  BPM in green. Unplayable files are red.
- **Sequence line (bottom).** The *only* arranging surface. Drag tracks in
  (insert anywhere, repeats welcome), drag entries to reorder, X removes.
  Each chip shows the track's tempo or a click-to-tap **needs beats** badge.
  The header reports **ready to render** or how many entries still need
  beats. Every change autosaves.
- **Beat-tap editor (central).** Opened from a library row or a chip's
  badge: play the track, tap the **↓ arrow** on each beat; a least-squares
  fit averages the taps into an exact tempo + downbeat (shown live). Click
  the waveform to add/remove a mark; trim handles; "save" persists the
  marks for good; "save to library" exports the trimmed WAV.

There is deliberately **no** transport for performing, no per-track volume,
no transition picker — the engine owns every mixing decision (zero knobs).

---

## 5. Finished functionality

### 5.1 Library  ✅
Folder tree of `.wav`/`.mp3`, filesystem-managed; drag-to-move with
spring-loaded folders; inline rename; delete; background playability probe
(red rows); per-row preview; per-row pencil → beat-tap editor; fitted-BPM
badge on tapped tracks.

### 5.2 Sequence line + project file  ✅
Ordered lane with repeats; insert/reorder/remove by drag; chips with tempo /
needs-beats badges; ready-to-render report. The sequence persists as a plain
one-path-per-line file (`sequence.txt` next to the user config), autosaved
on every change, restored on launch. Library renames/moves retarget entries;
deletes purge them.

### 5.3 Beat-tap, cached for good  ✅
The tap flow (play, tap ↓ per beat, least-squares grid fit → exact tempo +
downbeat) is a first-class library action. Marks persist per track
(`beats.txt`, stored with their sample rate, rescaled for a different output
device), survive restarts and renames/moves, and die with deletes. A track
is tapped **once, ever**.

### 5.4 Audio plumbing  ✅ (survives from the prior build)
cpal output + ring; symphonia decode (wav/mp3 → device rate, stereo); WAV
write; bundled MP3 encoder; varispeed playback; the mixer and its voices
(the clip editor borrows one as its audition slot); the whip/wiki scratch
DSP (now an *engine-internal* instrument); offline BPM detection (a rough
hint only — tapped beats are the source of truth).

### 5.5 Identity  ✅
CRT amber/green, landing-page palette, Space Mono + Bungee, scanline
overlay; slim brand bar.

---

## 6. Backlog (specced, not built) — the engine

In priority order; each story's acceptance criteria are its test spec.

1. **Naive auto-mix render** 🔜 (8 pts) — *the value seam.* First entry sets
   the master tempo; each entry contributes one deterministically-picked
   8–16-bar phrase-aligned section, varispeeded, loudness-normalized,
   butt-joined on phrase boundaries; WAV lands in the library. Repeat
   entries pick different material. Same input twice → identical bytes.
2. **Transition scheduler** 🔜 (3 pts) — seeded mix of swaps (~70%), hard
   cuts (~25%), short musical-length fades (~5%); everything stays on the
   phrase grid.
3. **Engine-placed scratches + fader chops** 🔜 (8 pts) — whip/wiki flurries
   from onset-rich slices of the sequence's own tracks; grid-locked starts,
   un-quantized seeded internal timing; clustered density; ~50 ms chops.
4. **Bass drops** 🔜 (3 pts) — ~16/hour scaled to length; low band ducks
   ≥10 dB for 1–16 s; restore exactly on a downbeat.
5. **Energy-arc shaping** 🔜 (5 pts) — section choice, gain, chop/drop
   placement bent toward ~6–8 min waves inside ~0.4–0.7 of peak; gentle
   low-end warmth over the back half. The user's order is never changed.
6. **Bit-identical determinism** 🔜 (3 pts) — same sequence → same SHA-256,
   across runs and macOS/Linux/Windows; golden-mix fixture test in CI.

### Release & site 🔜
Point termkrush.com (Porkbun DNS) at GitHub Pages; Buy-Me-A-Coffee in
README + site; first-release dry-run with an rc tag; **v0.1.0 "krush"**
release marker — lands only after the auto-mix MVP stories (1–6 above plus
the shipped surfaces) are accepted.

---

## 7. Retired (built, then superseded)  🗑

Kept so the history is legible; **not** part of the product:

- **Two decks, crossfader, deck sync/cue** — replaced by pads (2026-06-07).
- **Tracker step-grid arrange** — replaced by the looper (2026-06-07).
- **The ratatui TUI** — replaced by the egui GUI (2026-06-08, deleted).
- **Pads (loop/scratch/one-shot kinds, per-pad volume/activation), the
  master timeline + block editing, launch-quantized recording, the scratch
  platter and all scratch *performance* input** — replaced wholesale by the
  sequence line + mix grammar engine (2026-06-11). The whip/wiki sound model
  survives *inside* the engine; launch quantization survives as engine
  behavior (everything starts on a phrase boundary — it just never asks).

---

## 8. Out of scope

Pads, decks, crossfaders, timelines, or any performance surface; scratch
performance input; re-roll / variation seeds (determinism is strict); any
mixing knob (volume, EQ, transition choice, section choice, target length —
if a default is wrong we fix the grammar, not add a dial); in-app downloads;
pitch-preserving sync (varispeed is the sound); streaming; stems; networked
or cloud sessions; web / mobile.
