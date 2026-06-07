# Manual smoke checklist

The headless tests (`cargo test --workspace`) cover the engine + the input/
render mechanics, but some things only a human at a real terminal can confirm.
Run `cargo run --release` with a few tracks in `crate_root` and walk this:

## Library
- [ ] Tracks (and folders) list; `↑/↓` browse, `/` filters, `enter` on a folder
      navigates in, `..` goes back up.
- [ ] `l` (or `enter`) loads the highlighted track onto the focused pad; the
      cell shows it after a brief `loading…`.
- [ ] `x` delete (confirm), `R` rename, `m`/`p` move — all reflect on disk.

## Pads (8, no DJ tile)
- [ ] `space` (or `1`–`8`) plays the focused pad and you **hear** the clip;
      there are eight pads and no DJ cat.
- [ ] Focus moves with `tab` / `shift-tab` and `←`/`→`; **`↑`/`↓` = volume** of
      the focused pad (audible). On the library, `↑`/`↓` browse the list.
- [ ] `;` cycles kind (1shot / loop / scratch); `f` activates/deactivates with
      an audible fade.
- [ ] `u` unloads the focused pad (goes silent + empty).

## Tempo / sync
- [ ] Loading the first track silently sets the master tempo (no prompt); later
      loops play **in time** with it automatically; `{`/`}` speed the whole mix
      up/down and everything stays locked.

## Scratch
- [ ] On a scratch pad, `j` (wiki) and `k` (whip) sound like rubs; `P` records
      a phrase by tapping, `j` replays it, `C` clears.

## Edit modal (`e`)
- [ ] The clip bar + cursor move smoothly with `←/→` (shift = coarse); `i`/`o`
      set in/out you can hear; `x` truncates.

## Timeline
- [ ] A **persistent TIMELINE strip** sits across the top (8 lanes P1–P8, bar
      `|` separators, playhead `▶`/`:`); the library + pads are below it.
- [ ] `t` opens the full editor: cursor + step toggling feel right; `v`..`v`
      draws a loop region; `x` cuts
      the end; `space` plays/**pauses** (resumes from position), `backspace`
      stops/rewinds; `w` renders a WAV into the library.

## Render / export
- [ ] Rendered `mix-*.wav` and exported `*.mp3` play correctly in another
      player and sound like the session.
