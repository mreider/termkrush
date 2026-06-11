# Inception

> **Auto-mix pivot (2026-06-11).** Pads and the master timeline are retired.
> The user no longer arranges or performs anything. TermKrush becomes a
> **deterministic auto-mixer**: you drop tracks from the library onto a single
> **sequence line** in the order you want (the same track may appear at
> positions 1, 3, and 5), you tap beats once per track (the existing beat-mark
> flow), and the engine renders a continuous mix by applying a **mix grammar**
> measured from a real reference mix — one master tempo, 8–16-bar phrase
> sections, equal-loudness swaps on phrase boundaries, loose human
> micro-gestures (scratches, fader chops), bass drops, and a breathing energy
> arc. **Zero knobs**: sequence + beat marks is the entire input. **Strictly
> deterministic**: the random seed is derived from the input itself, so the
> same sequence renders the same mix bit-for-bit; there is no re-roll. The
> egui front-end and CRT identity stay; the UI simplifies to library +
> sequence line + beat-tap + render. The engine survives (decode, varispeed,
> beat-grid fit, mixer, render); pads, scratch performance input, the
> timeline, and launch-quantized recording are removed.

> **Pivot (2026-06-07).** The original two-deck turntable model was wrong. Based
> on user feedback we went back: there are **no decks**. Everything became a
> **pad**; the tool was a pad-based, timeline-arranged scratch/loop mixer.
> (Superseded by the 2026-06-11 auto-mix pivot above.)
>
> **Looper model (2026-06-07).** The arrangement became a master-timeline tape
> you performed onto with launch-quantized pad triggers, then edited and
> rendered. (Superseded by the 2026-06-11 auto-mix pivot above.)
>
> **GUI pivot (2026-06-08).** The terminal UI was retired in favor of a native,
> cross-platform **egui** desktop app — single binary, Win/Mac/Linux, CRT
> amber/green identity modernized, mouse-driven, no modal dialogs. **This pivot
> stands**: the auto-mix product lives in the same egui shell.

## The user

Any sort of idiot — someone who wants an awesome mix with old-school
scratching but has no DJ skills, no turntables, and (new with this pivot)
**no arranging skills and no patience to learn any**. They keep mp3s in
folders. The only musical act we ask of them is taste: pick the tracks, pick
the order. Tapping along to a song to mark its beats is the entire skill
ceiling.

## The goal

When we ship, this user can:

1. Build a **track list** — folders of audio (WAV native; import/export MP3),
   managed on the filesystem, browsed in the app.
2. **Mark beats** on each track they intend to use: tap along, and the
   existing least-squares grid fit locks an exact tempo + downbeat for that
   track. Marks are cached per track; a track is tapped once, ever.
3. Drop tracks onto the **sequence line** — a single ordered lane, the only
   arranging surface in the product. Repeats are allowed and encouraged
   (a track can be entries 1, 3, and 5; the engine picks different material
   from it each time).
4. Hit **render** and get a continuous mix, written to the library as WAV
   (MP3 export), produced by the **mix grammar engine**:
   - **One master tempo.** The first track in the sequence sets it; every
     other track varispeeds to that grid (pitch rides, like a platter). The
     grid never moves for the whole mix. Half-time-feel sections are allowed;
     tempo changes are not.
   - **Phrase sections.** Each sequence entry contributes a section of
     8–16 bars (occasionally up to 32, matching the reference distribution —
     median ~15 bars, one section per ~40 s). The engine picks
     **which** bars of the track to use, phrase-aligned to that track's own
     downbeats; repeat entries get different material.
   - **Equal-loudness swaps.** Every track is loudness-normalized at analysis
     time. The default transition is a swap on a phrase boundary at matched
     loudness (median step 0 dB); roughly a quarter of transitions are hard
     cuts used as punctuation; ramped fades are rare (~1 in 20). The engine
     schedules which transition each boundary gets.
   - **Macro quantized, micro human.** Scratch flurries (1–2 s, built from
     onset-rich slices of the sequence's own tracks) and short fader chops
     (~50 ms) are placed by the engine. Their *sections* land on the grid;
     their internal timing is deliberately **not** quantized — jitter comes
     from the input-derived seed, so it is loose but reproducible. Density
     matches the reference: a handful of scratch passages per hour, clustered
     into a stretch of the mix, often leaning on beat 2.
   - **Bass drops.** At tension points (roughly 16 per hour of output) the
     low band ducks >10 dB for 1–16 s, bar-quantized, and slams back on the
     one.
   - **Energy waves.** Section choice, gain, chop density, and drop placement
     are shaped so the loudness arc oscillates (~6–8 min period between
     roughly 0.4–0.7 of peak) instead of ramping, with the spectral balance
     allowed to warm (more low end) over the back half.
5. **Reproduce and revise.** The sequence (ordered track refs + cached beat
   marks) is the project file. Re-rendering the same sequence yields the
   identical mix; editing the order and re-rendering is the entire revision
   loop.

No pads, no timeline, no performing, no mixing decisions. The user curates;
the engine executes the craft.

## The reason

The pad/timeline model still asked the user to *be* an arranger and a
performer — gentler than decks, but a skill gate all the same. Then we
analyzed a genuinely great hour-long mix and found the craft is a small,
measurable grammar: one tempo, phrase blocks, equal-loudness swaps, loose
micro-gestures, bass drops, breathing energy. If the grammar fits in a page
of findings, it fits in an engine — and then no user should have to perform
it. Auto-DJ features in players do crossfades between songs; nothing renders
a *crafted* old-school scratch mix from a playlist. That's the gap.

## Success

A non-DJ picks ~10 tracks, taps beats for each, orders them on the sequence
line, hits render, and a listener can't tell the result from a hand-made
mix: no audible seams, never out of beat, and it has the life — scratches,
drops, dynamics — of the reference.

The smallest signal we'd believe: feed the engine the same source tracks as
the reference mix and A/B the two — if ours holds up side-by-side, the
grammar works. Second signal: identical input renders a bit-for-bit identical
file, on every platform.

Beyond that: stars/forks, buymeacoffee, and people posting rendered mixes.

## Constraints

- **Zero knobs.** The inputs are the sequence and the beat marks. Nothing
  else is user-controllable — no length dial, no density sliders. (If the
  grammar's defaults are wrong, we fix the grammar, not add a knob.)
- **Strict determinism.** Same input → identical output. All randomness is
  seeded from the input (track content + order + beat marks); no wall clock,
  no platform-dependent floating-point shortcuts in the render path.
- **Single binary.** No required runtime deps beyond the OS audio stack;
  MP3 encoder bundled.
- **WAV native, MP3 import + export. Filesystem-managed library.**
- **Varispeed, not pitch-preserving.** Pitch rides with tempo, like a real
  platter. (The time-stretch engine stays in the tree as a future option.)
- **Cross-platform, MIT, solo-maintainable**, egui GUI, CRT amber/green
  identity.

## Out of scope

- Pads, decks, crossfaders, the master timeline, launch-quantized recording,
  clip-block editing — every performance/arranging surface except the
  sequence line.
- Scratch *performance* input (whip/wiki tapping by the user). The engine
  places all scratches; the whip/wiki sound model survives inside the engine.
- Re-roll / variation seeds / "take numbers" — determinism is strict.
- Any mixing knob: volume, EQ, transition choice, section choice, length.
- In-app URL / yt-dlp downloads; streaming services; stems / vocal
  isolation; networked or cloud-synced sessions; web / mobile.

## Note on the prior build

Survives and gets repurposed: library scan + filesystem management, decode,
WAV/MP3 I/O, the beat-tap + least-squares grid fit (now the *only* per-track
user labor, so its accuracy work was well spent), the varispeed engine, the
mixer, the render path, and the egui shell with its CRT styling.

Removed: the pad model (loop/scratch/one-shot kinds, per-pad volume,
activate/deactivate), scratch performance input and phrase recording, the
master timeline and block editing, launch quantization as a *user-facing*
concept (the engine still launches everything on phrase boundaries — it just
never asks).

New and central: the **mix grammar engine** — section picker, transition
scheduler, scratch/chop synthesizer, bass-drop placer, energy-arc shaper —
all deterministic, all tuned to the measured reference numbers above.
