# TermKrush — Product Spec

*A keyboard-and-mouse, single-binary scratch/loop mixer for people who can't DJ.
Drop some loops, scratch over a beat by dragging a platter, arrange it on a
timeline, render a mix — with the software keeping everything in tempo.*

This spec describes the product as it stands today and what remains. It reflects
three pivots recorded in `.am/inception.md`:

1. **No decks — everything is a pad** (2026-06-07).
2. **Looper timeline** — you *perform* the arrangement instead of hand-toggling a
   step grid (2026-06-07).
3. **GUI pivot** — the terminal UI is being retired for a native **egui desktop
   app**, mouse-first (2026-06-08).

Status legend: **✅ done** (built; GUI pieces are delivered and awaiting the PM's
acceptance) · **🔜 backlog** (specced, not built) · **🗑 retired** (built once,
superseded by a pivot).

---

## 1. What it is

A non-DJ keeps audio files in folders. They load loops and one-shots onto pads,
set a scratch sound on a platter, and build a mix by performing into a timeline.
The app handles timing automatically (first loop sets the tempo; everything locks
to it), so nothing is ever off-beat. One binary, cross-platform (Win/Mac/Linux),
MIT, CRT amber/green identity.

**Run it:** `scripts/dev-run.sh gui` (the default; auto-builds). `scripts/dev-run.sh tui`
still launches the legacy terminal UI until the GUI reaches parity.

---

## 2. Architecture

- **`termkrush-core`** — the headless engine, zero UI dependencies: the mixer,
  sampler/scratch/jog voices, clip trim, BPM detect + cache, varispeed loop sync,
  launch-quantization clock, the library (filesystem), audio decode/encode/resample,
  cpal output, and the free-track arrangement model. Unit-tested in isolation.
- **`src/gui`** — the **egui/eframe** desktop front-end (current). View + input only;
  drives the engine. Audio is pumped to the cpal ring each frame.
- **`src/tui`** — the legacy **ratatui** terminal UI. Still functional behind
  `--tui`; **scheduled for deletion** once the GUI is at parity. 🗑(retiring)

Both front-ends share the same engine, so the pivot threw away no DSP.

---

## 3. Finished functionality

### 3.1 Library (file browser)  ✅
- Folder tree of audio files (`.wav` / `.mp3`), filesystem-managed — drop files in
  the directory and they appear; one level of subfolders.
- **Drag a track onto a pad** to load it (background decode, never blocks the UI).
- **Drag a track into a folder** to move it; **＋folder** to create one; **⬆..** to
  go up.
- **Double-click a track** to rename inline (no modal).
- Select a track + **🗑** to delete, **▶** to preview (plays once; click again to stop).
- **Unplayable files are flagged red** — a cheap background probe
  (`audio::probe_playable`, container/codec check, no full decode) runs per folder.

### 3.2 Pads  ✅
Eight pads. Each loaded pad cell has:
- **▶/⏸** play / pause (toggle; re-triggering never stacks voices).
- **Kind** selector — **1shot / loop / scratch** (click; clearer than a drag for a
  3-way toggle).
- **Volume** slider (per-pad gain, soft de-zipper).
- **on/off** toggle — activate/deactivate with a soft fade.
- **clear** (empties the pad), **export** (writes the trimmed clip to the library as
  WAV), **edit** (opens the clip editor).
- Empty pads show a "drag a track here" hint.

### 3.3 Clip editor  ✅
- Opens inline in the central panel (a focused mode, not a modal); **done** returns.
- **Real waveform** (`mixer.pad_peaks` min/max downsample).
- **Draggable ◀ in / ▶ out handles** set the trim live; the selected region is amber,
  the rest dim. (No zoom window — with a mouse the handles are precise on the full
  waveform.)
- **▶ play selection** auditions the trimmed region (click to stop).
- **export** writes the trimmed WAV to the library. Trim is non-destructive.

### 3.4 Scratch  ✅
- A bottom **SCRATCH platter**: **drag a track onto it** to arm the source.
- **Drag the platter left/right to scratch** — drag speed sets the jog velocity
  (right = *wiki* / forward, left = *whip* / backward); a held-still platter is
  silent, like a real one.
- **Hold ←/→ to jog** — works natively because the GUI (winit) reports real key-up,
  which the terminal could not do.
- Engine: a `JogVoice` in the mixer — a position-controlled platter read with linear
  interpolation (pitch rides speed), a **persistent playhead** (a `<` then `>`
  continues where it left off), clamped at the clip edges. An amber playhead line
  tracks the position.
- Also present in the engine from the earlier model: whip/wiki primitives, pivot/onset
  detection, and tap-to-build scratch phrases (`Build-a-scratch-phrase`). These feed
  the future scratch-record-to-timeline.

### 3.5 Timeline / arrangement  ✅ (engine) · 🔜 (UI)
- **Engine model — `termkrush-core::arrangement`** ✅: free, DAW-style **tracks** (not
  bound to pads) holding **blocks** — a clip's samples placed at a start frame.
  `add_track` / `add_block` / `move_block` / `remove_block` / `total_frames` /
  `render()` (sums every block at its position into one buffer). Headless-tested.
- **Looper capture engine** ✅ (from the looper pivot): a launch-quantization clock
  (master bar clock; triggers land on the next bar, never mid-bar) and an
  arrangement render-to-WAV path.
- **The GUI timeline editor is not built yet** — see backlog §4.1.

### 3.6 Audio engine  ✅
- **Mixer / master bus**: sums sampler voices, scratch voices, the library preview,
  and the jog platter; master gain with de-zipper ramp; master pause.
- **Pad voices**: one-shot, **loop** (repeats), and scratch playback over the trimmed
  region with per-pad gain + activation envelope.
- **Automatic tempo**: **the first loaded track sets the master BPM**; loops
  **varispeed** to it (pitch rides, platter feel) — no prompt. **Global speed** nudge
  moves every loop together. BPM is detected offline on load and **cached per file**.
- **Launch quantization**: triggers fire on the next bar boundary.
- **Library preview** and **live scratch jog** voices (unity-gain, summed on top).
- **I/O**: cpal output stream; symphonia decode (wav/mp3, resampled to the device
  rate, folded to stereo); WAV write; **MP3 export** (bundled encoder, no external
  tools); offline time-stretch engine present but **not** used for loop sync.

### 3.7 Identity  ✅
- **CRT amber/green**, matching the landing page palette (`index.html`): cream `--ink`
  body text, `--amber` / `--green` accents, `--bg` ground, `--line` borders,
  `--dim` muted, red for unplayable. Monospace throughout.

### 3.8 Controls reference (current GUI)
- **Mouse-first**, no modal dialogs — inline buttons + fields, drag-and-drop.
- **Drag**: track→pad (load), track→folder (move), track→platter (arm scratch),
  the platter↔ (scratch), clip handles (trim).
- **Double-click**: rename a track.
- **Keyboard** (minimal): **←/→** jog the scratch platter; per-widget buttons handle
  the rest. (Cmd-C / Cmd-V for timeline blocks arrives with the timeline editor.)

---

## 4. Backlog (specced, not built)

### 4.1 GUI free-track timeline editor  🔜 (epic `gui`, 8 pts)
The visual half of the timeline, on top of the finished arrangement model:
- Tracks as horizontal lanes; clips as **blocks** positioned by time; **add/remove
  tracks**.
- **Drag a clip/pad onto a track**; **drag blocks** to move; drag block edges to trim;
  **Cmd-C / Cmd-V** to copy/paste a block.
- **Transport** (play/pause with a moving playhead); **render** to WAV; **tempo ±** /
  **master ±**.
- *Open design questions for the PM:* snap to the bar grid or free placement? does
  playback route through the mixer or sum alongside it? where does paste land?

### 4.2 Scratch — record to the timeline  🔜 (part of `gui` platter story)
Capture a performed jog gesture as a timeline block at a cued position. Depends on
§4.1. (The live scratch *feel* is already done.)

### 4.3 Session save / load — `.tekr`  🔜 (epic `session`, 8 pts)
- **Save on quit**: write a `.tekr` (JSON) into the launch directory — per-pad source
  path / kind / trim / gain / active / bpm / phrase; the timeline arrangement; master
  bpm + gain. Paths, not audio.
- **Load** (`L`): list `.tekr` files in the launch dir, restore everything by
  re-decoding the stored source paths; missing sources flag red / skip.

### 4.4 Record the timeline into a pad  🔜 (epic `looper`, 5 pts)
From the timeline, bounce the arrangement (or its loop region) into a chosen pad;
"are you sure?" overwrite confirm if the pad isn't empty.

### 4.5 Fade-in / fade-out on timeline blocks  🔜 (epic `looper`)
Per-block fades on the timeline.

### 4.6 Retire the TUI  🔜 (chore, epic `gui`)
Delete `src/tui` once the GUI reaches parity (and the legacy ratatui/terminal code +
its tests go with it).

### 4.7 Release & site  🔜
- **Refresh GitHub Pages** for the pad/GUI model; **point termkrush.com (Porkbun) DNS**
  at GitHub Pages; **wire Buy-Me-A-Coffee** into the README + site.
- **First-release dry-run** with an rc tag; **v0.1.0 "krush"** release marker — lands
  only after the MVP stories are accepted.

### 4.8 YouTube → WAV import  🔜 (note: tension with "no in-app downloads")
A filed feature to import a YouTube song as WAV. The inception lists in-app downloads
as out of scope; keep or drop is a PM call.

---

## 5. Retired (built, then superseded)  🗑
Kept here so the history is legible; **not** part of the current product:
- **Two decks, crossfader, deck sync/cue, auto-fade, turntable platter visuals** —
  replaced wholesale by pads (2026-06-07).
- **Tracker step-grid arrange** (place/region/cut by hand) — replaced by the performed
  looper timeline (2026-06-07).
- **The ratatui TUI** and its keyboard-command surface — being replaced by the egui
  GUI (2026-06-08), pending deletion (§4.6).
- **8-bit DJ-cat mascot** — dropped during the pad rebuild.

---

## 6. Out of scope
Decks / crossfader as the interaction model; pitch-preserving sync (we chose
varispeed); GUI-less streaming; stems / vocal isolation; networked or cloud sessions.
In-app downloads beyond the (debated) YouTube→WAV import.
