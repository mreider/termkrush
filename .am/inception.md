# Inception

## The user

A bedroom DJ, hacker, or terminal dweller who wants to mix tracks without leaving the shell. They keep an mp3 crate locally, they grab new tracks from YouTube / SoundCloud / Bandcamp, and they want enough turntable-style control (sync, scratch, fade, a couple of FX) to do a real warm-up set — without firing up Rekordbox or Serato. They are comfortable on the keyboard, they like CRT aesthetics, they enjoy the fact that the whole thing is one binary.

## The goal

When we ship, this user can:

1. Drop into a TUI from their terminal.
2. Pull mp3s into a local crate via direct URL or yt-dlp.
3. Load tracks onto multiple decks (up to four).
4. See BPM per deck, sync one to another, time-stretch without pitch drift.
5. Crossfade smoothly between decks.
6. Scratch over a marked segment of a deck using keyboard jog while another deck rides.
7. Apply minimal FX (filter, echo, reverb) per deck.
8. Record the master mix to mp3 or wav.

All of it in a single static binary, no GUI, no DAW, no cloud account.

## The reason

DJ software is heavy, GUI-bound, and locked to specific hardware ecosystems. There is no terminal-native, keyboard-first, file-based DJ tool that treats mp3s as a crate and the keyboard as a controller. Existing CLI audio tools (sox, ffmpeg, mpv) are not designed for live performance. This fills the gap, and it does it in a form factor that is small enough for one developer to maintain.

## Success

A user can sit down, run `termkrush`, load two tracks they downloaded that morning, sync the BPMs, and crossfade between them in a way that sounds good — without reading docs. First release marker is the moment that becomes true.

Beyond that:

- Stars / forks on GitHub indicate interest.
- buymeacoffee transactions indicate genuine appreciation.
- People posting recorded sets in issues / discussions indicates the tool is being used in anger.

## Constraints

- **Single static binary.** No required runtime deps beyond the OS audio stack. `yt-dlp` is optional and shelled out to (if installed) — never bundled.
- **Cross-platform.** macOS (arm64 / amd64), Linux (arm64 / amd64), Windows (amd64). One Rust codebase, GitHub Actions release pipeline.
- **MIT license.** Public repo. No proprietary deps.
- **No release until usable.** First release (`v0.1.0 spins`) requires: two decks, BPM detect, sync, crossfade, local mp3 load — end-to-end.
- **Solo maintainership.** Scope must stay walkable by one person.
- **Visual identity.** CRT amber/green palette, Bungee + Space Mono typography on the landing page; the TUI should echo the same feel.

## Out of scope

- GUI / web UI / browser frontend.
- Mobile.
- MIDI controller integration (icebox).
- Streaming services (Spotify / Apple Music) — strictly local files.
- DAW-style timeline editing, multi-track production.
- Stems / vocal isolation.
- Pre-fade cue listen on a second audio interface (icebox).
- Networked / collab sessions.
- Cloud sync of the crate.
