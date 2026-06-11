# Manual smoke checklist

Headless tests cover the mechanics; these need a human with a display and
speakers. Run `scripts/dev-run.sh build && scripts/dev-run.sh gui` with a few
tracks in `crate_root`.

## The surfaces

Three, always visible, no modes: **library** (left), **beat-tap editor**
(center, when open), **sequence line** (bottom). A slim brand bar up top.
CRT amber/green everywhere, scanlines, no modal dialogs.

## Library
- [ ] Launch shows your folders + tracks; subfolders open on click; `.. (up)`
      walks back; the root is the ceiling.
- [ ] ▶ on a row previews (click again stops); only one thing sounds at a
      time.
- [ ] Double-click renames inline; drag a track over a folder and *hold* —
      it springs open; drop moves the file; drag onto the trash deletes.
- [ ] Unplayable files show red and their buttons disable.
- [ ] A track you've tapped shows its BPM in green at the row's end.

## Beat-tap editor (the pencil on any row)
- [ ] Pencil opens the editor with the track's waveform (and any previously
      saved marks already on it).
- [ ] ▶ plays; tapping the **↓ arrow** drops a green mark at the playhead on
      each tap; the label live-updates "N beats · ≈X BPM" from the
      least-squares fit.
- [ ] Click the waveform to add/remove a mark; **clear** wipes them.
- [ ] Drag the in/out handles to trim; **save to library** writes the
      trimmed WAV (appears in the library and gets probed).
- [ ] **save** closes the editor and persists the marks: quit, relaunch,
      reopen the editor — the marks are still there.
- [ ] Rename or move the track in the library — its marks (and green BPM
      badge) follow it.

## Sequence line
- [ ] Drag a track from the library into the lane: a numbered chip appears.
      Drop on an existing chip inserts *before* it; drop on the empty tail
      appends.
- [ ] The same track can be added at several positions.
- [ ] Drag a chip onto another chip to reorder; X removes one entry only.
- [ ] Untapped entries show an amber **needs beats** badge — clicking it
      opens the beat-tap editor for that track. Tapped entries show their
      BPM in green.
- [ ] The header reads "N entries need beats" until every entry is tapped,
      then **ready to render** in green.
- [ ] Quit and relaunch: the sequence comes back exactly (order + repeats).
- [ ] Rename/move a sequenced track: the chip follows. Delete it: its chips
      vanish.

## Render *(engine stories pending — extend as they land)*
- [ ] (naive render story) Render with every entry tapped → a `mix-*.wav`
      appears in the library, beats locked to one tempo, no audible seam at
      section boundaries.
- [ ] (determinism story) Render the same sequence twice → identical files
      (same SHA-256).
