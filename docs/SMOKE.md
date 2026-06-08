# Manual smoke checklist

Headless tests cover the mechanics; these need a human at a real terminal.
Run `scripts/dev-run.sh build && scripts/dev-run.sh tui` with a few tracks in
`crate_root`.

## The whole control surface
Five controls, reused by context. The header's second line always shows what
they do *right now*.

- **Arrows** — move. On a pad, `↑`/`↓` = that pad's volume.
- **Tab / Shift-Tab** — jump area: Library → Pads → Timeline.
- **Space** — play (the focused pad; the arrangement on the Timeline).
- **Enter** — open the context menu (its first item is the common action, so
  Enter-then-Enter does the obvious thing). While recording a scratch phrase,
  Enter taps a whip instead.
- **Esc** — back / cancel (closes the menu or editor; again = quit).

## Walkthrough
- [ ] The **library is home** on launch, nothing highlighted. With nothing
      highlighted, `1`–`8` **jump** to a pad, `0` to the timeline.
- [ ] `↑`/`↓` **highlight** a song. While a song is highlighted: `1`–`8`
      **load** it onto that pad (if that pad's full, it asks to overwrite),
      `Space` previews it, `r` rename, `Backspace` delete, `←`/`→` move. `Esc` **stops highlighting**
      (then `1`–`8` jump again); a 2nd `Esc` goes up a folder / quits.
- [ ] Move: a folder modal — `↑`/`↓` pick `(root)` or a folder,
      `n` new folder (type a name, `Enter`), `r` rename, `Backspace` delete
      (confirm), `Enter` moves the file there. One level of folders only.
- [ ] Pad: `Space` plays/pauses; `↑`/`↓` volume; `Enter` opens its menu —
      edit clip / **kind** (highlight it, `←`/`→` to change 1shot↔loop↔scratch) /
      on-off / **export** (WAV to the library) / **clear**. Empty pads have no
      menu — load from the library. (No load / save / save-over / mp3.)
- [ ] Scratch pad (set kind → scratch): the menu then also shows **rec phrase /
      clear phrase**; `Space` = wiki, `Enter` menu → record phrase, tap
      `Space`/`Enter`, then `Space` replays it.
- [ ] Clip editor: `←`/`→` move the active handle; **`+`/`-` zoom** the window
      (whole→10s→1s→100ms→10ms) so steps get as fine as you need; `Tab` switches
      in/out handle **and the view follows it** (so you can see/scroll to the
      end); the minimap line shows where you are; `Space` auditions ~1.5s AT the active handle (again stops), `Enter` snips, `Esc` closes.
- [ ] Timeline: `←`/`→` step, `↑`/`↓` lane, `Enter` toggles a hit, `Space`
      plays/pauses; `Enter` menu → place / cut / region / clear / render / tempo / master.
- [ ] No help screen or M key — Enter opens the context menu everywhere; the hint line + menus explain everything.

## Render / export
- [ ] Rendered `mix-*.wav` and exported `*.mp3` play correctly elsewhere and
      sound like the session.
