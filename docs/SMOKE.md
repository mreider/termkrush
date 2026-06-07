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

## Pads
- [ ] `1`–`7` / `j` trigger and you **hear** the clip.
- [ ] `;` cycles kind (1shot / loop / scratch); `f` activates/deactivates with
      an audible fade; `-`/`=` change that pad's volume audibly.
- [ ] `u` unloads the focused pad (goes silent + empty).

## Tempo / sync
- [ ] Loading the first track with a BPM pops "Sync all tracks to N BPM?";
      `y` makes loops play **in time** together; `{`/`}` speed the whole mix
      up/down and everything stays locked.

## Scratch
- [ ] On a scratch pad, `j` (wiki) and `k` (whip) sound like rubs; `P` records
      a phrase by tapping, `j` replays it, `C` clears.

## Edit modal (`e`)
- [ ] The clip bar + cursor move smoothly with `←/→` (shift = coarse); `i`/`o`
      set in/out you can hear; `x` truncates.

## Timeline (`t`)
- [ ] Cursor + step toggling feel right; `v`..`v` draws a loop region; `x` cuts
      the end; `space` plays/**pauses** (resumes from position), `backspace`
      stops/rewinds; `w` renders a WAV into the library.

## Render / export
- [ ] Rendered `mix-*.wav` and exported `*.mp3` play correctly in another
      player and sound like the session.
