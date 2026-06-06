//! Terminal user interface (ratatui + crossterm).
//!
//! The split here is deliberate so the UI is testable without a tty:
//!
//! - [`App`] holds UI state and maps input events to [`Action`]s — pure,
//!   unit-tested directly.
//! - [`draw`] renders the current state into a ratatui [`Frame`] — pure,
//!   tested headlessly via `TestBackend`.
//! - [`run`] owns the messy parts: alternate screen, raw mode, the redraw
//!   loop, and (via [`TerminalGuard`]) restoring the terminal on the way
//!   out — including when the event loop panics, since Drop runs during
//!   unwind.
//!
//! Colors follow the CRT palette from the design: amber wordmark, green
//! tagline, near-black background.

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::audio::{AudioOutput, DecodedAudio};
use crate::clip::Clip;
use crate::config::Config;
use crate::deck::{Deck, DeckState};
use crate::library::Crate;
use crate::mix::{Mixer, Pattern, DECKS, PADS};

/// Display labels for the decks, indexed by deck number.
const DECK_LABELS: [&str; DECKS] = ["A", "B"];

/// Per-keypress gain nudge (linear), for both deck and master.
const GAIN_NUDGE: f32 = 0.05;

/// Selectable auto-fade durations (seconds); `space` cycles through them.
const FADE_SECS: [f32; 4] = [1.0, 2.0, 4.0, 8.0];

/// Seek amounts (seconds): the per-deck seek keys jump `SEEK_JUMP`, with
/// Shift held they jump `SEEK_FAR`; `,`/`.` scrub the focused deck finely.
const SEEK_JUMP: f64 = 5.0;
const SEEK_FAR: f64 = 30.0;
const SEEK_SCRUB: f64 = 0.1;

/// CRT amber, `#ffb000` — the wordmark and accents.
pub const AMBER: Color = Color::Rgb(0xff, 0xb0, 0x00);
/// CRT green, `#45f07d` — secondary text.
pub const GREEN: Color = Color::Rgb(0x45, 0xf0, 0x7d);
/// Near-black background, `#060907`.
pub const BG: Color = Color::Rgb(0x06, 0x09, 0x07);

/// Redraw cap: poll for input up to this long, giving ~30 Hz when idle.
const FRAME: Duration = Duration::from_millis(33);

/// Where a freshly-decoded track is headed.
#[derive(Debug, Clone, Copy)]
enum LoadTarget {
    Deck(usize),
    Pad(usize),
}

/// A decoded track handed from a background decode thread back to the UI
/// loop. Decoding + resampling a full track is slow, so it runs off the
/// event-loop thread and the result is applied here — the UI never freezes.
struct Decoded {
    target: LoadTarget,
    track: DecodedAudio,
    path: PathBuf,
    bpm: Option<f32>,
}

/// Decode `path` (and optionally detect its BPM) on a background thread,
/// posting the result to `tx`. Never blocks the caller.
fn spawn_decode(
    target: LoadTarget,
    path: PathBuf,
    target_rate: u32,
    detect: bool,
    cached_bpm: Option<f32>,
    tx: Sender<Decoded>,
) {
    std::thread::spawn(
        move || match crate::audio::decode_file(&path, target_rate) {
            Ok(track) => {
                // Use the cached BPM when we have one (skips re-analysis);
                // otherwise detect afresh when asked.
                let bpm = cached_bpm.or_else(|| {
                    if detect {
                        crate::audio::detect_bpm(&track.samples, track.channels, track.sample_rate)
                    } else {
                        None
                    }
                });
                let _ = tx.send(Decoded {
                    target,
                    track,
                    path,
                    bpm,
                });
            }
            Err(e) => tracing::error!(error = %e, path = %path.display(), "decode failed"),
        },
    );
}

/// What an input event asks the app to do. Deck transport is applied
/// directly to deck A (left-hand keys) or deck B (right-hand keys) inside
/// `on_key`; the variant is returned so the caller (and the tests) can
/// observe what happened. `OpenFile`/`LoadSelected` are the exceptions:
/// loading a track is I/O, so `on_key` only signals intent and the event
/// loop performs the decode. `focus` only steers crate loads and fine scrub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    ToggleHelp,
    ConfirmQuit,
    PlayPause,
    Stop,
    OpenFile,
    DeckGain,
    MasterGain,
    Seek,
    CrateNav,
    Filter,
    LoadSelected,
    Focus,
    Crossfade,
    ToggleCrate,
    TriggerPad,
    AssignPad,
    Bpm,
    Mark,
    Record,
}

/// A focusable cell of the control surface (plus the crate browser on the
/// left). One is focused at a time; the shared action cluster acts on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Crate,
    Deck(usize), // 0 = A, 1 = B
    MixSoft,
    MixHard,
    Pad(usize), // 0..PADS
    Dj,
}

impl Default for Focus {
    fn default() -> Self {
        Focus::Deck(0)
    }
}

/// Arrow-key direction for grid focus navigation.
#[derive(Debug, Clone, Copy)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// A normalized gamepad input — decoupled from `gilrs` so the mapping is
/// unit-testable without hardware. The runtime translates raw pad events
/// into these; [`App::on_pad`] applies them via the same focus→act model the
/// keyboard uses (Xbox is the preferred controller; the keyboard mirrors it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PadInput {
    FocusDeckA, // LB
    FocusDeckB, // RB
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    FaceA, // primary   (play / trigger / load)
    FaceB, // secondary (cue / fade-to-B)
    FaceX, // mark / assign
    FaceY, // alt
    FadeA, // LT — auto-fade toward A
    FadeB, // RT — auto-fade toward B
    Crossfade(f32),
    Jog(f32),
    Quit, // Start
    Help, // Back / View
}

/// UI state for the shell: the decks + master bus (owned by [`Mixer`]),
/// which deck has focus, and the browsable local crate.
#[derive(Debug, Default)]
pub struct App {
    pub show_help: bool,
    pub should_quit: bool,
    /// When true, the "Quit? (y/n)" confirmation modal is open and captures
    /// input until the user confirms (`y`) or cancels (`n`/Esc).
    pub confirm_quit: bool,
    /// Index into [`FADE_SECS`] — the auto-fade duration `space` cycles.
    fade_idx: usize,
    pub mixer: Mixer,
    /// The focused cell — what the action cluster acts on. `Tab` steps
    /// through every cell; arrows move the focus box across the grid.
    focus: Focus,
    /// The deck that transitions / load / jog act around when the focused
    /// cell isn't itself a deck. Updated whenever a deck is focused.
    last_deck: usize,
    /// Scanned local crate of mp3s.
    crate_lib: Crate,
    /// Selection index into the *filtered* crate view.
    crate_sel: usize,
    /// `Some(query)` while the `/` filter is active; `None` otherwise.
    filter: Option<String>,
    /// Set when the user picks a track to load; the event loop performs the
    /// decode (I/O) and clears it.
    pending_load: Option<PathBuf>,
    /// When true the crate browser is hidden, giving the decks full width.
    crate_collapsed: bool,
    /// Recently loaded tracks (most-recent first), shown as the "Loaded"
    /// shortlist for quick re-assignment to a deck.
    recent: Vec<PathBuf>,
    /// Set when the user assigns a clip to a sampler pad; the event loop
    /// decodes it (I/O) and clears it. `(pad index, path)`.
    pending_pad_assign: Option<(usize, PathBuf)>,
    /// Per-deck "a track is decoding in the background" flag, for the
    /// `loading…` indicator. Cleared when the decoded track arrives.
    loading: [bool; DECKS],
    /// Per-deck record-in mark (playhead frame) while recording a clip; set
    /// by mark-in, consumed by mark-out.
    record_in: [Option<usize>; DECKS],
    /// Clips recorded this session (most recent last), awaiting assignment
    /// to a pad.
    recordings: Vec<Clip>,
    /// Cache of detected BPM per file path, so reloading a track skips
    /// re-analysis.
    bpm_cache: HashMap<PathBuf, f32>,
}

/// How many recently-loaded tracks the "Loaded" shortlist keeps.
const RECENT_CAP: usize = 6;

impl App {
    pub fn new() -> Self {
        App::default()
    }

    /// Install a freshly-scanned crate, resetting the selection.
    pub fn set_crate(&mut self, crate_lib: Crate) {
        self.crate_lib = crate_lib;
        self.crate_sel = 0;
    }

    /// The deck the controls act around: the focused deck, else the last
    /// deck focused (when the focus is on the mixer / pads / crate / DJ).
    fn active_deck(&self) -> usize {
        match self.focus {
            Focus::Deck(i) => i,
            _ => self.last_deck,
        }
    }

    /// The active deck, shared / mutable.
    fn focused(&self) -> &Deck {
        self.mixer.deck(self.active_deck())
    }
    fn focused_mut(&mut self) -> &mut Deck {
        let i = self.active_deck();
        self.mixer.deck_mut(i)
    }

    /// The currently-selected auto-fade duration in seconds.
    pub fn fade_secs(&self) -> f32 {
        FADE_SECS[self.fade_idx % FADE_SECS.len()]
    }

    /// The focused cell — for rendering + tests.
    pub fn focus_cell(&self) -> Focus {
        self.focus
    }

    /// Which deck the load path targets (the active deck).
    pub fn focus(&self) -> usize {
        self.active_deck()
    }

    /// `true` if a clip pad is focused (compat shim for the pad readout).
    pub fn clips_focused(&self) -> bool {
        matches!(self.focus, Focus::Pad(_))
    }
    /// The focused pad slot (0 when no pad is focused).
    pub fn clip_sel(&self) -> usize {
        match self.focus {
            Focus::Pad(i) => i,
            _ => 0,
        }
    }

    // ---- focus navigation ----

    /// Set focus, remembering the deck whenever one is focused so the
    /// transitions/load/jog have a sensible target from any cell.
    fn set_focus(&mut self, f: Focus) {
        if let Focus::Deck(i) = f {
            self.last_deck = i;
        }
        self.focus = f;
    }

    /// Linear `Tab` order: crate, decks, mixer cells, pads, DJ.
    fn tab_order() -> Vec<Focus> {
        let mut v = vec![
            Focus::Crate,
            Focus::Deck(0),
            Focus::Deck(1),
            Focus::MixSoft,
            Focus::MixHard,
        ];
        v.extend((0..PADS).map(Focus::Pad));
        v.push(Focus::Dj);
        v
    }

    /// `Tab`: step to the next cell, wrapping.
    fn cycle_target(&mut self) -> Action {
        let order = Self::tab_order();
        let idx = order.iter().position(|&f| f == self.focus).unwrap_or(0);
        self.set_focus(order[(idx + 1) % order.len()]);
        Action::Focus
    }

    /// Grid coordinates of a cell: column 0 is the crate (full-height left),
    /// columns 1-2 are the two control columns; rows 0-5 top to bottom.
    fn focus_rc(f: Focus) -> (i32, i32) {
        match f {
            Focus::Crate => (0, 0),
            Focus::Deck(0) => (0, 1),
            Focus::Deck(_) => (0, 2),
            Focus::MixSoft => (1, 1),
            Focus::MixHard => (1, 2),
            Focus::Dj => (5, 2),
            Focus::Pad(i) => (i as i32 / 2 + 2, i as i32 % 2 + 1),
        }
    }
    fn rc_focus(row: i32, col: i32) -> Focus {
        if col <= 0 {
            return Focus::Crate;
        }
        match (row, col) {
            (0, 1) => Focus::Deck(0),
            (0, 2) => Focus::Deck(1),
            (1, 1) => Focus::MixSoft,
            (1, 2) => Focus::MixHard,
            (5, 2) => Focus::Dj,
            (r, c) => {
                let i = ((r - 2) * 2 + (c - 1)) as usize;
                if i < PADS {
                    Focus::Pad(i)
                } else {
                    Focus::Dj
                }
            }
        }
    }

    /// Arrow navigation across the grid. On the crate, up/down browse the
    /// list; right enters the grid. Elsewhere it walks the 2-column grid,
    /// and left from the left column returns to the crate.
    fn move_focus(&mut self, dir: Dir) -> Action {
        if self.focus == Focus::Crate {
            return match dir {
                Dir::Up => {
                    self.sel_up();
                    Action::CrateNav
                }
                Dir::Down => {
                    self.sel_down();
                    Action::CrateNav
                }
                Dir::Right => {
                    self.set_focus(Focus::Deck(0));
                    Action::Focus
                }
                Dir::Left => Action::None,
            };
        }
        let (mut r, mut c) = Self::focus_rc(self.focus);
        match dir {
            Dir::Up => r = (r - 1).max(0),
            Dir::Down => r = (r + 1).min(5),
            Dir::Left => c -= 1,
            Dir::Right => c = (c + 1).min(2),
        }
        if c <= 0 {
            self.set_focus(Focus::Crate);
        } else {
            self.set_focus(Self::rc_focus(r, c));
        }
        Action::Focus
    }

    /// Load the highlighted crate track into the active deck.
    fn load_selected(&mut self) -> Action {
        self.pending_load = self.selected_path();
        if self.pending_load.is_some() {
            Action::LoadSelected
        } else {
            Action::None
        }
    }

    /// Apply a gamepad input via the same focus→act model as the keyboard.
    /// Pure of hardware (takes a normalized [`PadInput`]), so it's testable;
    /// the runtime translates raw `gilrs` events into these.
    pub fn on_pad(&mut self, input: PadInput) -> Action {
        use PadInput::*;
        // The quit modal captures confirm/cancel first.
        if self.confirm_quit {
            return match input {
                FaceA | Quit => {
                    self.should_quit = true;
                    Action::Quit
                }
                FaceB => {
                    self.confirm_quit = false;
                    Action::ConfirmQuit
                }
                _ => Action::None,
            };
        }
        match input {
            FocusDeckA => {
                self.set_focus(Focus::Deck(0));
                Action::Focus
            }
            FocusDeckB => {
                self.set_focus(Focus::Deck(1));
                Action::Focus
            }
            DpadUp => self.move_focus(Dir::Up),
            DpadDown => self.move_focus(Dir::Down),
            DpadLeft => self.move_focus(Dir::Left),
            DpadRight => self.move_focus(Dir::Right),
            FaceA => self.act_primary(),
            FaceB => self.act_secondary(),
            FaceX => self.act_mark_in(),
            FaceY => self.act_mark_out(),
            FadeA => {
                self.mixer.autofade_to(-1.0, self.fade_secs());
                Action::Crossfade
            }
            FadeB => {
                self.mixer.autofade_to(1.0, self.fade_secs());
                Action::Crossfade
            }
            // Right stick = continuous crossfade (the platter/fader).
            Crossfade(x) => {
                self.mixer.cut_to(x.clamp(-1.0, 1.0));
                Action::Crossfade
            }
            // Left stick = jog/scratch the focused deck.
            Jog(x) => {
                let amt = x.clamp(-1.0, 1.0) as f64 * SEEK_SCRUB;
                self.focused_mut().seek_by(amt);
                Action::Seek
            }
            Quit => {
                self.confirm_quit = true;
                Action::ConfirmQuit
            }
            Help => {
                self.show_help = !self.show_help;
                Action::ToggleHelp
            }
        }
    }

    // ---- the context-sensitive action cluster (j / k / l / ;) ----

    fn act_primary(&mut self) -> Action {
        match self.focus {
            Focus::Deck(i) => {
                self.mixer.deck_mut(i).toggle();
                Action::PlayPause
            }
            Focus::Pad(i) => self.fire_pad(i),
            Focus::MixSoft => {
                self.mixer.autofade_to(-1.0, self.fade_secs()); // fade to A
                Action::Crossfade
            }
            Focus::MixHard => {
                self.mixer.cut_to(-1.0); // hard cut to A
                Action::Crossfade
            }
            Focus::Crate => self.load_selected(),
            Focus::Dj => Action::None,
        }
    }

    fn act_secondary(&mut self) -> Action {
        match self.focus {
            Focus::Deck(i) => {
                self.mixer.deck_mut(i).stop();
                Action::Stop
            }
            Focus::MixSoft => {
                self.mixer.autofade_to(1.0, self.fade_secs()); // fade to B
                Action::Crossfade
            }
            Focus::MixHard => {
                self.mixer.cut_to(1.0); // hard cut to B
                Action::Crossfade
            }
            // On a pad: drop the most-recent recording onto this slot.
            Focus::Pad(i) => match self.recordings.last() {
                Some(clip) => {
                    self.mixer.assign_pad(i, clip.samples.clone());
                    self.mixer.set_pad_bpm(i, clip.bpm);
                    Action::AssignPad
                }
                None => Action::None,
            },
            _ => Action::None, // deck/crate/dj have no secondary here
        }
    }

    fn act_mark_in(&mut self) -> Action {
        match self.focus {
            // On a deck: mark the record-in point at the current playhead.
            Focus::Deck(i) => {
                self.record_in[i] = Some(self.mixer.deck(i).position_frames());
                Action::Mark
            }
            // On a pad: assign the highlighted crate track to it.
            Focus::Pad(i) => match self.selected_path() {
                Some(p) => {
                    self.pending_pad_assign = Some((i, p));
                    Action::AssignPad
                }
                None => Action::None,
            },
            _ => Action::None,
        }
    }

    fn act_mark_out(&mut self) -> Action {
        match self.focus {
            // On a deck: with an in-point set, capture the region [in, out)
            // off the deck into a recorded clip.
            Focus::Deck(i) => match self.record_in[i].take() {
                Some(start) => {
                    let deck = self.mixer.deck(i);
                    let end = deck.position_frames();
                    let samples = deck.capture(start, end);
                    if samples.len() >= 2 {
                        let name =
                            format!("Deck {} clip {}", DECK_LABELS[i], self.recordings.len() + 1);
                        self.recordings.push(Clip::new(samples, deck.bpm(), name));
                        Action::Mark
                    } else {
                        Action::None // empty region
                    }
                }
                None => Action::None, // no in-point yet
            },
            // On a pad: cycle its playback pattern (Straight/Cut/BabyScratch).
            Focus::Pad(i) => {
                self.mixer.cycle_pad_pattern(i);
                Action::Mark
            }
            _ => Action::None,
        }
    }

    /// Clips recorded this session, oldest first.
    pub fn recordings(&self) -> &[Clip] {
        &self.recordings
    }

    /// Trigger pad `i`, beat-matching to the active deck's tempo when the
    /// pad has auto-BPM on (else native rate, with its pattern).
    fn fire_pad(&mut self, i: usize) -> Action {
        let target = self.mixer.deck(self.active_deck()).bpm().unwrap_or(0.0);
        self.mixer.trigger_pad_synced(i, target);
        Action::TriggerPad
    }

    /// `b` — toggle auto-BPM on the focused pad.
    fn toggle_pad_autobpm(&mut self) -> Action {
        if let Focus::Pad(i) = self.focus {
            self.mixer.toggle_pad_autobpm(i);
            Action::Mark
        } else {
            Action::None
        }
    }

    /// `r` — toggle the live-mix recorder. Arming captures the master output
    /// (decks + active pads). Disarming turns the capture into a clip: onto
    /// the focused pad if one is focused, else into the recordings stash.
    fn toggle_record(&mut self) -> Action {
        if self.mixer.is_recording() {
            let samples = self.mixer.take_recording();
            if samples.len() >= 2 {
                let bpm = self.mixer.deck(self.active_deck()).bpm();
                if let Focus::Pad(i) = self.focus {
                    self.mixer.assign_pad(i, samples);
                    self.mixer.set_pad_bpm(i, bpm);
                } else {
                    let name = format!("Mix resample {}", self.recordings.len() + 1);
                    self.recordings.push(Clip::new(samples, bpm, name));
                }
            }
        } else {
            self.mixer.arm_record();
        }
        Action::Record
    }

    /// Whether deck `i` is armed (an in-point is set, awaiting mark-out).
    fn recording(&self, i: usize) -> bool {
        self.record_in.get(i).map(|m| m.is_some()).unwrap_or(false)
    }

    /// `,`/`.` — tempo of the focused cell. On a deck it nudges varispeed
    /// (pitch rides; effective BPM = base × speed); on a pad it nudges the
    /// pad's stored BPM. `fine` = a tenth of the step.
    fn nudge_tempo(&mut self, up: bool, fine: bool) -> Action {
        let sign = if up { 1.0 } else { -1.0 };
        match self.focus {
            Focus::Deck(i) => {
                let step = if fine { 0.001 } else { 0.01 }; // 0.1% / 1% per press
                self.mixer.deck_mut(i).nudge_speed(sign * step);
                Action::Bpm
            }
            Focus::Pad(i) => {
                let step = if fine { 0.1 } else { 1.0 };
                self.mixer.nudge_pad_bpm(i, sign * step);
                Action::Bpm
            }
            _ => Action::None,
        }
    }

    // ---- the context-sensitive value keys (w / s): deck volume ----

    fn value_up(&mut self, fine: bool) -> Action {
        match self.focus {
            Focus::Deck(i) => {
                self.mixer.deck_mut(i).nudge_gain(GAIN_NUDGE);
                Action::DeckGain
            }
            Focus::Pad(i) => {
                self.mixer.nudge_pad_out(i, Self::trim_step(fine));
                Action::Mark
            }
            _ => Action::None,
        }
    }

    fn value_down(&mut self, fine: bool) -> Action {
        match self.focus {
            Focus::Deck(i) => {
                self.mixer.deck_mut(i).nudge_gain(-GAIN_NUDGE);
                Action::DeckGain
            }
            Focus::Pad(i) => {
                self.mixer.nudge_pad_out(i, -Self::trim_step(fine));
                Action::Mark
            }
            _ => Action::None,
        }
    }

    /// Trim nudge in frames: ~0.1s coarse, ~0.01s fine (44.1k-ish).
    fn trim_step(fine: bool) -> i64 {
        if fine {
            441
        } else {
            4410
        }
    }

    /// `a`/`d` — jog. On a deck it scrubs the playhead; on a pad it nudges
    /// the trim in-point (non-destructive). `coarse` = the larger step.
    fn jog(&mut self, forward: bool, coarse: bool) -> Action {
        let sign = if forward { 1.0 } else { -1.0 };
        match self.focus {
            Focus::Deck(_) => {
                let amt = if coarse { SEEK_FAR } else { SEEK_SCRUB };
                self.focused_mut().seek_by(sign * amt);
                Action::Seek
            }
            Focus::Pad(i) => {
                let d = Self::trim_step(!coarse) * if forward { 1 } else { -1 };
                self.mixer.nudge_pad_in(i, d);
                Action::Mark
            }
            _ => Action::None,
        }
    }

    /// The current filter query (empty string when not filtering).
    fn filter_query(&self) -> &str {
        self.filter.as_deref().unwrap_or("")
    }

    /// The crate entries currently visible (after the filter).
    fn visible(&self) -> Vec<&crate::library::CrateEntry> {
        self.crate_lib.filtered(self.filter_query())
    }

    /// Path of the highlighted crate entry, if any.
    fn selected_path(&self) -> Option<PathBuf> {
        self.visible().get(self.crate_sel).map(|e| e.path.clone())
    }

    /// Take the pending load request (set when a track is chosen). The
    /// event loop calls this and performs the decode.
    pub fn take_pending_load(&mut self) -> Option<PathBuf> {
        self.pending_load.take()
    }

    /// Record a track as freshly loaded — front of the "Loaded" shortlist,
    /// de-duplicated, capped at [`RECENT_CAP`]. Called by the event loop
    /// after a successful load.
    pub fn note_loaded(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(RECENT_CAP);
    }

    /// The "Loaded" shortlist, most-recent first.
    pub fn recent(&self) -> &[PathBuf] {
        &self.recent
    }

    /// Take the pending pad assignment (set when a clip is assigned to a
    /// pad). The event loop decodes it and assigns it to the mixer.
    pub fn take_pending_pad_assign(&mut self) -> Option<(usize, PathBuf)> {
        self.pending_pad_assign.take()
    }

    /// Apply a background-decoded track: onto a deck (with BPM + recents,
    /// clearing the loading flag) or onto a sampler pad.
    fn place_decoded(&mut self, d: Decoded) {
        match d.target {
            LoadTarget::Deck(i) => {
                let name = d
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("track")
                    .to_string();
                self.mixer.deck_mut(i).load_named(d.track, name);
                if let Some(b) = d.bpm {
                    self.mixer.deck_mut(i).set_bpm(b);
                    self.bpm_cache.insert(d.path.clone(), b); // remember for reloads
                }
                self.note_loaded(d.path);
                if i < DECKS {
                    self.loading[i] = false;
                }
            }
            LoadTarget::Pad(i) => self.mixer.assign_pad(i, d.track.samples),
        }
    }

    /// Whether deck `i` is currently decoding a track in the background.
    fn is_loading(&self, i: usize) -> bool {
        self.loading.get(i).copied().unwrap_or(false)
    }

    fn sel_down(&mut self) {
        let n = self.visible().len();
        if n > 0 {
            self.crate_sel = (self.crate_sel + 1).min(n - 1);
        }
    }

    fn sel_up(&mut self) {
        self.crate_sel = self.crate_sel.saturating_sub(1);
    }

    /// Map a terminal event to an [`Action`], mutating state as needed.
    pub fn on_event(&mut self, ev: &Event) -> Action {
        match ev {
            Event::Key(key) => self.on_key(*key),
            _ => Action::None,
        }
    }

    /// Map a key press to an [`Action`]. Release/repeat events are ignored
    /// (Windows reports both).
    pub fn on_key(&mut self, key: KeyEvent) -> Action {
        if key.kind == KeyEventKind::Release {
            return Action::None;
        }
        // Ctrl-C is the unconditional hard escape hatch — works from any
        // mode, including the quit modal.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return Action::Quit;
        }
        // The quit modal is modal: only y / n / Esc matter; everything else
        // is swallowed so a stray key can't act on the decks behind it.
        if self.confirm_quit {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.should_quit = true;
                    return Action::Quit;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirm_quit = false;
                    return Action::ConfirmQuit;
                }
                _ => return Action::None,
            }
        }
        // While the filter is open, keystrokes edit the query / navigate the
        // list rather than driving transport.
        if self.filter.is_some() {
            return self.on_key_filter(key);
        }
        // Ergonomic, deck-symmetric layout: the LEFT hand drives deck A,
        // the RIGHT hand mirrors it for deck B, the crossfader sits between
        // the hands, and the crate/global keys stay off the play cluster.
        // Keys are chosen by finger position, not by what letter the action
        // starts with. See `keymap` docs / the help overlay.
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match (key.code, key.modifiers) {
            // ---- global / out of the play cluster ----
            // `q` no longer quits outright — it opens the confirm modal, so a
            // fat-fingered `q` mid-set can't drop you out.
            (KeyCode::Char('q'), _) => {
                self.confirm_quit = true;
                Action::ConfirmQuit
            }
            (KeyCode::Char('?'), _) => {
                self.show_help = !self.show_help;
                Action::ToggleHelp
            }
            (KeyCode::Esc, _) if self.show_help => {
                self.show_help = false;
                Action::ToggleHelp
            }
            // Esc (with no overlay open) asks to quit.
            (KeyCode::Esc, _) => {
                self.confirm_quit = true;
                Action::ConfirmQuit
            }

            // ---- action cluster (right hand) — acts on the focused target ----
            // Deck focused: j play/pause, k cue/stop. Clips focused: j trigger
            // the selected slot, l assign the highlighted crate track to it.
            // (mark-in/out and pattern/auto-bpm fill in as those features land.)
            (KeyCode::Char('j'), _) => self.act_primary(),
            (KeyCode::Char('k'), _) => self.act_secondary(),
            (KeyCode::Char('l'), _) => self.act_mark_in(),
            (KeyCode::Char(';'), _) => self.act_mark_out(),

            // ---- select / value (left hand) ----
            // Deck: w/s = volume. Pad: w/s = trim out-point ± (shift = fine).
            (KeyCode::Char('w'), _) => self.value_up(shift),
            (KeyCode::Char('s'), _) => self.value_down(shift),

            // ---- jog (left hand); shift = coarse ----
            // Deck: scrub. Pad: trim in-point ∓.
            (KeyCode::Char('a'), _) => self.jog(false, shift),
            (KeyCode::Char('d'), _) => self.jog(true, shift),

            // ---- deck transitions — between the hands (index inner reach) ----
            // lowercase = instant hard cut; Shift = hands-free auto-fade over
            // the selected duration; space cycles that duration.
            (KeyCode::Char('g'), _) => {
                self.mixer.cut_to(-1.0); // hard cut to A
                Action::Crossfade
            }
            (KeyCode::Char('h'), _) => {
                self.mixer.cut_to(1.0); // hard cut to B
                Action::Crossfade
            }
            (KeyCode::Char('G'), _) => {
                self.mixer.autofade_to(-1.0, self.fade_secs()); // auto-fade to A
                Action::Crossfade
            }
            (KeyCode::Char('H'), _) => {
                self.mixer.autofade_to(1.0, self.fade_secs()); // auto-fade to B
                Action::Crossfade
            }
            (KeyCode::Char(' '), _) => {
                self.fade_idx = (self.fade_idx + 1) % FADE_SECS.len(); // cycle duration
                Action::Crossfade
            }

            // ---- master ----
            (KeyCode::Char('['), _) => {
                self.mixer.nudge_master(-GAIN_NUDGE);
                Action::MasterGain
            }
            (KeyCode::Char(']'), _) => {
                self.mixer.nudge_master(GAIN_NUDGE);
                Action::MasterGain
            }

            // ---- BPM of the focused target: ,/. nudge ∓1 (shift = ∓0.1) ----
            (KeyCode::Char(','), _) => self.nudge_tempo(false, shift),
            (KeyCode::Char('.'), _) => self.nudge_tempo(true, shift),

            // ---- focus + crate browser ----
            (KeyCode::Tab, _) => self.cycle_target(),
            (KeyCode::Char('/'), _) => {
                self.filter = Some(String::new());
                self.crate_sel = 0;
                Action::Filter
            }
            (KeyCode::Up, _) => self.move_focus(Dir::Up),
            (KeyCode::Down, _) => self.move_focus(Dir::Down),
            (KeyCode::Left, _) => self.move_focus(Dir::Left),
            (KeyCode::Right, _) => self.move_focus(Dir::Right),
            (KeyCode::Enter, _) => self.load_selected(),
            (KeyCode::Char('z'), _) => self.crate_collapse_toggle(),
            (KeyCode::Char('\\'), _) => Action::OpenFile, // load demo into focused deck
            (KeyCode::Char('r'), _) => self.toggle_record(), // resample the live mix
            (KeyCode::Char('b'), _) => self.toggle_pad_autobpm(), // beat-match this pad

            // ---- direct clip triggers (quick, always live) ----
            (KeyCode::Char(c @ '1'..='7'), _) => {
                let pad = c.to_digit(10).unwrap() as usize - 1;
                self.fire_pad(pad)
            }

            _ => Action::None,
        }
    }

    /// Toggle the crate browser's visibility (helper so the key arm stays
    /// a one-liner).
    fn crate_collapse_toggle(&mut self) -> Action {
        self.crate_collapsed = !self.crate_collapsed;
        Action::ToggleCrate
    }

    /// Key handling while the `/` filter is open: type to narrow, arrows
    /// (or while empty, nothing) navigate, Enter loads the highlight and
    /// closes the filter, Esc clears and closes it.
    fn on_key_filter(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.filter = None;
                self.crate_sel = 0;
                Action::Filter
            }
            KeyCode::Enter => {
                self.pending_load = self.selected_path();
                self.filter = None;
                if self.pending_load.is_some() {
                    Action::LoadSelected
                } else {
                    Action::None
                }
            }
            KeyCode::Backspace => {
                if let Some(q) = self.filter.as_mut() {
                    q.pop();
                }
                self.crate_sel = 0;
                Action::Filter
            }
            KeyCode::Up => {
                self.sel_up();
                Action::CrateNav
            }
            KeyCode::Down => {
                self.sel_down();
                Action::CrateNav
            }
            KeyCode::Char(c) => {
                if let Some(q) = self.filter.as_mut() {
                    q.push(c);
                }
                self.crate_sel = 0;
                Action::Filter
            }
            _ => Action::None,
        }
    }
}

/// Render the current state. The splash centers the wordmark and tagline
/// over the CRT background; when help is toggled, a centered overlay is
/// drawn on top.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    // Paint the whole screen with the CRT background.
    f.render_widget(Block::new().style(Style::default().bg(BG)), area);

    // Wordmark and the key hint near the top, the deck panel centered below.
    let rows = Layout::vertical([
        Constraint::Length(1), // top padding
        Constraint::Length(1), // wordmark
        Constraint::Length(1), // hint row
        Constraint::Min(0),    // body
    ])
    .split(area);

    let wordmark = Paragraph::new("TermKrush")
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(AMBER)
                .bg(BG)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(wordmark, rows[1]);

    // Transport hint row — green accent. Left hand = A, right hand = B.
    let tagline =
        Paragraph::new("tab focus   j play  k cue   a/d jog   g/h cut  G/H fade   ? help")
            .alignment(Alignment::Center)
            .style(Style::default().fg(GREEN).bg(BG));
    f.render_widget(tagline, rows[2]);

    // Body: an optional crate browser on the left, the mixer area on the
    // right. The crate collapses (key `c`) to give the decks full width.
    let mixer_area = if app.crate_collapsed {
        rows[3]
    } else {
        let body = Layout::horizontal([Constraint::Length(32), Constraint::Min(0)]).split(rows[3]);
        // Left column: the crate browser, with the "Loaded" shortlist beneath.
        let left = Layout::vertical([Constraint::Min(0), Constraint::Length(8)]).split(body[0]);
        draw_crate_panel(f, left[0], app);
        draw_loaded_panel(f, left[1], app);
        body[1]
    };

    // Control surface: a 2-column grid of equal cells —
    //   [Deck A | Deck B] [Mix·soft | Mix·hard] [Pad1|Pad2] … [Pad7 | DJ].
    let grid_rows = Layout::vertical([Constraint::Ratio(1, 6); 6]).split(mixer_area);
    let split2 = |r: Rect| {
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(r)
    };

    // Row 0 — the two decks.
    let r = split2(grid_rows[0]);
    for i in 0..DECKS {
        let focused = app.focus_cell() == Focus::Deck(i);
        draw_deck_cell(
            f,
            r[i],
            DECK_LABELS[i],
            app.mixer.deck(i),
            focused,
            app.is_loading(i),
            app.recording(i),
        );
    }

    // Row 1 — mixer: soft (auto-fade) | hard (cut + master).
    let r = split2(grid_rows[1]);
    let m = app.mixer.master_gain();
    let soft_title = if app.mixer.is_recording() {
        "Mix · soft  ●REC"
    } else {
        "Mix · soft"
    };
    draw_cell(
        f,
        r[0],
        soft_title,
        vec![
            Line::from(blend_state(app)),
            Line::from(format!("auto-fade {:.0}s", app.fade_secs())),
        ],
        app.focus_cell() == Focus::MixSoft,
    );
    draw_cell(
        f,
        r[1],
        "Mix · hard",
        vec![
            Line::from(blend_state(app)),
            Line::from(format!("master {m:.2}  {}", fmt_db(m))),
        ],
        app.focus_cell() == Focus::MixHard,
    );

    // Rows 2-4 — pads 1-6; row 5 — pad 7 + the DJ placeholder.
    for (row, base) in [(2usize, 0usize), (3, 2), (4, 4)] {
        let r = split2(grid_rows[row]);
        draw_pad_cell(f, r[0], app, base);
        draw_pad_cell(f, r[1], app, base + 1);
    }
    let r = split2(grid_rows[5]);
    draw_pad_cell(f, r[0], app, 6); // pad 7
    draw_cell(f, r[1], "DJ", dj_lines(app), app.focus_cell() == Focus::Dj);

    if app.show_help {
        draw_help(f, area);
    }
    // The quit modal sits on top of everything (including help).
    if app.confirm_quit {
        draw_quit_modal(f, area);
    }
}

/// A small centered "Quit?" confirmation modal. `y` quits, `n`/Esc cancels.
fn draw_quit_modal(f: &mut Frame, area: Rect) {
    let w = 34.min(area.width);
    let h = 5.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);
    f.render_widget(Clear, popup);
    let modal = Paragraph::new("\nQuit TermKrush?\n(y) yes    (n) no")
        .alignment(Alignment::Center)
        .block(
            Block::bordered()
                .border_style(Style::default().fg(AMBER))
                .title("Quit"),
        );
    f.render_widget(modal, popup);
}

/// The mixer row: a bordered panel with the crossfader fader graphic over
/// the master readout, sitting beneath the two decks.
/// A bordered grid cell with `title` and `body` lines; amber border when
/// focused, dim otherwise. The shared building block of the control grid.
fn draw_cell(f: &mut Frame, area: Rect, title: &str, body: Vec<Line>, focused: bool) {
    let border = deck_border(focused);
    let panel = Paragraph::new(body)
        .block(
            Block::bordered()
                .title(title.to_string())
                .style(Style::default().fg(border).bg(BG)),
        )
        .style(Style::default().fg(GREEN).bg(BG));
    f.render_widget(panel, area);
}

/// The deck-blend state shown in the mixer cells: which deck is live, or
/// the fade in progress.
fn blend_state(app: &App) -> String {
    let x = app.mixer.xfade_applied();
    if app.mixer.is_fading() {
        if app.mixer.xfade() >= x {
            "A → B".into()
        } else {
            "B → A".into()
        }
    } else if x <= -0.5 {
        "▶ A".into()
    } else if x >= 0.5 {
        "▶ B".into()
    } else {
        "A + B".into()
    }
}

/// Which bob frame the 8-bit DJ cat is on: it alternates once per beat off
/// the first playing deck's effective BPM, and rests (frame 0) when nothing
/// is playing or no tempo is known.
fn dj_frame(app: &App) -> usize {
    for i in 0..DECKS {
        let d = app.mixer.deck(i);
        if d.state() == DeckState::Playing {
            if let Some(bpm) = d.bpm() {
                let beats = d.position_secs() * bpm as f64 / 60.0;
                return (beats.floor() as i64).rem_euclid(2) as usize;
            }
        }
    }
    0
}

/// The DJ tile's two-line 8-bit cat for the current bob frame (low detail
/// on purpose). Frame 1 lifts the ears / opens the eyes a touch.
fn dj_lines(app: &App) -> Vec<Line<'static>> {
    let (face, deck) = if dj_frame(app) == 0 {
        ("  =^.^=", "  ♫ dj ♫")
    } else {
        ("  =^o^=", "  ♫ DJ ♫")
    };
    vec![Line::from(face), Line::from(deck)]
}

/// One sampler-pad cell: number + assigned/empty glyph + its BPM. Focused
/// (amber) when the Clip bank is focused and this is the selected slot.
/// A `[░░████░░]` bar showing the trimmed region `[in, out)` over the clip.
fn trim_bar(inp: usize, out: usize, len: usize, width: usize) -> String {
    if len == 0 || width == 0 {
        return "[]".into();
    }
    let lo = (inp * width / len).min(width);
    let hi = (out * width / len).min(width);
    let mut s = String::with_capacity(width + 2);
    s.push('[');
    for i in 0..width {
        s.push(if i >= lo && i < hi { '█' } else { '░' });
    }
    s.push(']');
    s
}

fn pattern_label(p: Pattern) -> &'static str {
    match p {
        Pattern::Straight => "play",
        Pattern::Cut => "cut",
        Pattern::BabyScratch => "scratch",
        Pattern::Transformer => "xform",
        Pattern::Stutter => "stutter",
        Pattern::Warble => "warble",
        Pattern::Reverse => "reverse",
    }
}

fn draw_pad_cell(f: &mut Frame, area: Rect, app: &App, pad: usize) {
    let focused = app.focus_cell() == Focus::Pad(pad);
    let loaded = app.mixer.pad_loaded(pad);
    let glyph = if loaded { '●' } else { '·' };
    // When focused with a clip, show the trim timeline; otherwise the BPM.
    let line2 = if focused && loaded {
        let (inp, out) = app.mixer.pad_trim(pad);
        let len = app.mixer.pad_clip_frames(pad);
        let w = (area.width as usize).saturating_sub(6).clamp(4, 12);
        format!("  {}", trim_bar(inp, out, len, w))
    } else {
        let bpm = app
            .mixer
            .pad_bpm(pad)
            .map(|b| format!("{b:.0} bpm"))
            .unwrap_or_else(|| "-- bpm".into());
        format!("  {bpm}")
    };
    let line1 = if loaded {
        let sync = if app.mixer.pad_autobpm(pad) {
            " sync"
        } else {
            ""
        };
        format!(
            "  {glyph} {}{sync}",
            pattern_label(app.mixer.pad_pattern(pad))
        )
    } else {
        format!("  {glyph}")
    };
    draw_cell(
        f,
        area,
        &format!("Pad {}", pad + 1),
        vec![Line::from(line1), Line::from(line2)],
        focused,
    );
}

/// One deck cell: title (with BPM), name + state, and a position bar / clock.
fn draw_deck_cell(
    f: &mut Frame,
    area: Rect,
    label: &str,
    deck: &Deck,
    focused: bool,
    loading: bool,
    recording: bool,
) {
    let marker = if focused { "▸ " } else { "" };
    let bpm = match deck.bpm() {
        Some(b) => format!("  {b:.0} BPM"),
        None => String::new(),
    };
    let title = format!("{marker}Deck {label}{bpm}");
    let inner = area.width.saturating_sub(2) as usize;
    let name = if loading {
        "⏳ loading…".to_string()
    } else {
        ellipsize(
            deck.display_name().unwrap_or("— no track —"),
            inner.saturating_sub(4),
        )
    };
    let state_word = match deck.state() {
        DeckState::Empty => "empty",
        DeckState::Loaded => "loaded",
        DeckState::Playing => "playing",
        DeckState::Paused => "paused",
        DeckState::Stopped => "stopped",
    };
    let elapsed = deck.position_secs();
    let total = deck.duration_secs();
    let frac = if total > 0.0 { elapsed / total } else { 0.0 };
    let bar_w = inner.saturating_sub(24).clamp(3, 16); // leave room for the clock
    let rec = if recording { "  ●REC" } else { "" };
    let spd = if (deck.speed() - 1.0).abs() > 1e-3 {
        format!("  {:+.0}%", (deck.speed() - 1.0) * 100.0)
    } else {
        String::new()
    };
    let body = vec![
        Line::from(Span::styled(
            format!(
                "  {} {}  {state_word}{rec}{spd}",
                transport_glyph(deck.state()),
                name
            ),
            Style::default().fg(AMBER),
        )),
        Line::from(format!(
            "  {}  {} / {}",
            progress_bar(frac, bar_w),
            fmt_clock(elapsed),
            fmt_clock(total)
        )),
    ];
    draw_cell(f, area, &title, body, focused);
}

/// Render the crate browser: a bordered, scrollable list of tracks with
/// the highlight at the current selection. The block title shows the
/// track count, or the live filter query while `/` is open.
fn draw_crate_panel(f: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible();
    let bc = deck_border(app.focus_cell() == Focus::Crate); // amber when focused
    let title = match &app.filter {
        Some(q) => format!("Crate  /{q}_  ({} match)", visible.len()),
        None => format!("Crate  ({} tracks)", app.crate_lib.len()),
    };

    // Empty crate (not filtering): show a wrapped how-to instead of a single
    // line that runs off the panel edge.
    if app.crate_lib.is_empty() && app.filter.is_none() {
        let help = Paragraph::new(
            "No tracks found.\n\nSet crate_root in your\nconfig.toml — see the\nREADME, then relaunch.",
        )
        .wrap(Wrap { trim: false })
        .block(
            Block::bordered()
                .title(title.clone())
                .border_style(Style::default().fg(bc))
                .style(Style::default().fg(GREEN).bg(BG)),
        )
        .style(Style::default().fg(GREEN).bg(BG));
        f.render_widget(help, area);
        return;
    }

    // Names wider than the panel get an ellipsis rather than a hard cut.
    // Width = inner minus the borders and the "▶ " highlight gutter.
    let name_w = (area.width as usize).saturating_sub(4);
    let items: Vec<ListItem> = if visible.is_empty() {
        vec![ListItem::new("(no matches)")]
    } else {
        visible
            .iter()
            .map(|e| ListItem::new(ellipsize(&e.name, name_w)))
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(title)
                .border_style(Style::default().fg(bc))
                .style(Style::default().fg(GREEN).bg(BG)),
        )
        .highlight_style(
            Style::default()
                .fg(AMBER)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
        .highlight_symbol("▶ ");

    // A local ListState carries the selection so the widget scrolls to keep
    // it visible; nothing to persist between frames for a single deck.
    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.crate_sel.min(visible.len().saturating_sub(1))));
    }
    f.render_stateful_widget(list, area, &mut state);
}

/// The "Loaded" shortlist: tracks pulled into the session this run, most
/// recent first. A quick reference for what's available to drop onto a
/// deck (focus with `tab`, then load from the crate / re-pick here).
fn draw_loaded_panel(f: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = if app.recent().is_empty() {
        vec![Line::from("  (none yet)")]
    } else {
        app.recent()
            .iter()
            .map(|p| {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("track");
                Line::from(format!("• {name}"))
            })
            .collect()
    };
    let panel = Paragraph::new(lines)
        .block(
            Block::bordered()
                .title("Loaded")
                .style(Style::default().fg(GREEN).bg(BG)),
        )
        .style(Style::default().fg(GREEN).bg(BG));
    f.render_widget(panel, area);
}

/// Truncate `s` to at most `max` display columns, ending in `…` when cut,
/// so long track names don't run off the panel edge.
fn ellipsize(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max == 0 {
        return s.to_string();
    }
    if max == 1 {
        return "…".to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// Format a linear gain as dBFS-relative decibels: `1.0 -> +0.0 dB`,
/// `0.5 -> -6.0 dB`, `0.0 -> -inf dB`.
fn fmt_db(gain: f32) -> String {
    if gain <= 0.0 {
        return "-inf dB".to_string();
    }
    format!("{:+.1} dB", 20.0 * gain.log10())
}

/// A `Rect` of size `w`x`h` centered within `area` (clamped to it).
fn centered_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

/// Deck panel border color: amber for the focused deck, dim for the rest
/// (per the design). Pure so the focus-color rule can be unit-tested.
fn deck_border(focused: bool) -> Color {
    if focused {
        AMBER
    } else {
        Color::DarkGray
    }
}

/// Transport indicator glyph for a deck state.
fn transport_glyph(state: DeckState) -> &'static str {
    match state {
        DeckState::Playing => "▶",
        DeckState::Paused => "⏸",
        DeckState::Stopped => "■",
        DeckState::Loaded => "⏏",
        DeckState::Empty => "·",
    }
}

/// A `[████░░░░]` progress bar `width` cells wide between the brackets,
/// filled proportionally to `frac` (clamped to `[0, 1]`).
fn progress_bar(frac: f64, width: usize) -> String {
    let filled = (frac.clamp(0.0, 1.0) * width as f64).round() as usize;
    let mut s = String::with_capacity(width + 2);
    s.push('[');
    for i in 0..width {
        s.push(if i < filled { '█' } else { '░' });
    }
    s.push(']');
    s
}

/// Format seconds as `mm:ss.s`.
fn fmt_clock(secs: f64) -> String {
    let secs = secs.max(0.0);
    let mins = (secs / 60.0).floor() as u64;
    let rem = secs - (mins as f64) * 60.0;
    format!("{mins:02}:{rem:04.1}")
}

/// A centered help overlay (stub: lists the keys it knows so far).
fn draw_help(f: &mut Frame, area: Rect) {
    let w = 58.min(area.width);
    let h = 23.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);
    // Keys are by finger position: left hand = deck A, right hand = deck B.
    let help = Paragraph::new(
        "Keys  —  focus a target, then act\n\n  focus       tab + arrows  (every cell; crate is left)\n  act  j play/trig  k cue/assign-rec  l mark-in/assign  ; mark-out/pattern\n  value       w / s     (deck volume · pad trim out)\n  jog         a / d     (deck scrub · pad trim in; shift = fine)\n\n  transition  g/h cut A/B   G/H auto-fade   space dur\n  master      [ / ]      tempo , / . (deck varispeed)\n  clips       1-7 trigger   r record mix   b beat-match pad\n\n  crate   / filter   ↑/↓ pick   enter load\n          \\ load demo   z hide crate\n  ?  help   esc/q quit   C-c force",
    )
    .block(
        Block::bordered()
            .title("Help")
            .style(Style::default().fg(AMBER).bg(BG)),
    )
    .style(Style::default().fg(GREEN).bg(BG));
    f.render_widget(help, popup);
}

/// Leave alternate screen, disable raw mode, show the cursor. Idempotent:
/// safe to call when already restored, so the RAII guard and the panic
/// hook can both run it.
fn restore() {
    let mut out = io::stdout();
    let _ = disable_raw_mode();
    let _ = execute!(out, LeaveAlternateScreen, ratatui::crossterm::cursor::Show);
}

/// RAII guard: restores the terminal when dropped — on a clean return and
/// during a panic unwind alike, so the user's shell is never left in raw
/// mode.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Wrap the current panic hook so the terminal is restored *before* the
/// crash is printed. The panic hook fires before unwinding (so before the
/// RAII guard's Drop), which is why restoring here matters: otherwise the
/// crash message would land in the alternate screen / raw mode. Chains to
/// the previously installed hook (the crash-logging hook from `logging`).
fn install_panic_restore() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        prev(info);
    }));
}

type Term = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, ratatui::crossterm::cursor::Hide)?;
    Terminal::new(CrosstermBackend::new(out))
}

/// Run the TUI event loop until the user quits. Restores the terminal on
/// the way out via [`TerminalGuard`].
pub fn run() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;
    install_panic_restore();
    let res = event_loop(&mut terminal);
    // _guard drops here (or during unwind) and restores the terminal.
    res
}

fn event_loop(terminal: &mut Term) -> io::Result<()> {
    let mut app = App::new();

    // Scan the configured crate root so the browser is populated at launch.
    let cfg = Config::load();
    let crate_lib = Crate::scan(&cfg.crate_root);
    tracing::info!(root = %cfg.crate_root.display(), tracks = crate_lib.len(), "crate scanned");
    app.set_crate(crate_lib);

    // Bring up the output device; degrade gracefully to a silent UI if it
    // is unavailable (headless CI, no device), exactly like `--test-tone`.
    // The ring (~93ms at 44.1k stereo) is kept topped by `pump` each frame,
    // so it also bounds transport latency — small enough that play/pause
    // feels responsive, large enough to ride out the ~33ms poll. A
    // dedicated audio thread for tighter latency is a later refinement.
    let (audio_out, mut producer) = match AudioOutput::start(1 << 13) {
        Ok((out, prod)) => (Some(out), Some(prod)),
        Err(e) => {
            tracing::warn!(error = %e, "audio output unavailable; running without sound");
            (None, None)
        }
    };
    let out_channels = audio_out.as_ref().map(|o| o.channels).unwrap_or(2);
    let target_rate = audio_out.as_ref().map(|o| o.sample_rate).unwrap_or(44_100);
    app.mixer.set_sample_rate(target_rate); // so auto-fades land in real seconds
    let mut scratch: Vec<f32> = Vec::new();

    // Background decode threads post finished tracks (with BPM) back here.
    let (load_tx, load_rx) = std::sync::mpsc::channel::<Decoded>();

    // Gamepad (Xbox preferred). Optional: absent or unsupported → keyboard
    // only. We poll its events each frame alongside the terminal's.
    let mut gilrs = gilrs::Gilrs::new().ok();

    tracing::info!(gamepad = gilrs.is_some(), "tui event loop started");
    while !app.should_quit {
        terminal.draw(|f| draw(f, &app))?;
        // Drain gamepad events (non-blocking) through the same action path.
        if let Some(g) = gilrs.as_mut() {
            while let Some(gilrs::Event { event, .. }) = g.next_event() {
                if let Some(input) = translate_pad(event) {
                    let action = app.on_pad(input);
                    apply_load_action(&mut app, action, target_rate, &load_tx);
                    apply_pad_assign(&mut app, action, target_rate, &load_tx);
                }
            }
        }
        // Poll up to one frame; redraw at least every FRAME, sooner on input.
        if event::poll(FRAME)? {
            let ev = event::read()?;
            let action = app.on_event(&ev);
            apply_load_action(&mut app, action, target_rate, &load_tx);
            apply_pad_assign(&mut app, action, target_rate, &load_tx);
        }
        // Apply any tracks that finished decoding in the background.
        while let Ok(decoded) = load_rx.try_recv() {
            app.place_decoded(decoded);
        }
        // Top up the output ring from the mixed decks. Done here in the UI
        // loop (not a separate thread) so the realtime cpal callback stays
        // lock-free; the ring covers the ~33ms between frames.
        if let Some(p) = producer.as_mut() {
            pump(&mut app.mixer, p, out_channels, &mut scratch);
        }
    }
    tracing::info!("tui event loop exited");
    drop(audio_out); // stop the stream before the terminal is restored
    Ok(())
}

/// Draw the mixed stereo output (both decks summed + master) and push it
/// into the output ring, mapping to the device's channel count (L/R, with
/// any extra channels silent and a mono device taking the left channel).
/// Writes only as many frames as the ring currently has room for, so it
/// never blocks.
fn pump(
    mixer: &mut Mixer,
    producer: &mut rtrb::Producer<f32>,
    channels: u16,
    scratch: &mut Vec<f32>,
) {
    let channels = channels.max(1) as usize;
    let frames = producer.slots() / channels;
    if frames == 0 {
        return;
    }
    scratch.resize(frames * 2, 0.0);
    mixer.fill_mix(scratch); // both decks summed + master gain
    for f in 0..frames {
        let (l, r) = (scratch[f * 2], scratch[f * 2 + 1]);
        for ch in 0..channels {
            let s = match ch {
                0 => l,
                1 => r,
                _ => 0.0,
            };
            let _ = producer.push(s); // room was reserved above
        }
    }
}

/// Carry out a load `action` — the event loop's load step, lifted out so
/// it can be exercised end-to-end in tests (no TTY/audio needed). On
/// `OpenFile` it loads the demo track into the focused deck; on
/// `LoadSelected` it loads the pending crate selection. Returns whether a
/// track was loaded, and records it in the "Loaded" shortlist on success.
fn apply_load_action(
    app: &mut App,
    action: Action,
    target_rate: u32,
    load_tx: &Sender<Decoded>,
) -> bool {
    let focus = app.focus();
    let path = match action {
        Action::OpenFile => demo_track_path(),
        Action::LoadSelected => match app.take_pending_load() {
            Some(p) => p,
            None => return false,
        },
        _ => return false,
    };
    // Decode off the UI thread so a long track / resample never freezes
    // input; the deck shows `loading…` until the result arrives. Skip BPM
    // re-analysis when this file's tempo is already cached.
    app.loading[focus] = true;
    let cached = app.bpm_cache.get(&path).copied();
    spawn_decode(
        LoadTarget::Deck(focus),
        path,
        target_rate,
        cached.is_none(),
        cached,
        load_tx.clone(),
    );
    true
}

/// Carry out an `AssignPad` action: decode the pending clip and assign it
/// to its sampler pad. Returns whether a clip was assigned. Lifted out of
/// the event loop so it's testable.
fn apply_pad_assign(
    app: &mut App,
    action: Action,
    target_rate: u32,
    load_tx: &Sender<Decoded>,
) -> bool {
    if action != Action::AssignPad {
        return false;
    }
    let Some((pad, path)) = app.take_pending_pad_assign() else {
        return false;
    };
    // Decode off-thread too — a long clip shouldn't freeze the UI either.
    spawn_decode(
        LoadTarget::Pad(pad),
        path,
        target_rate,
        false,
        None,
        load_tx.clone(),
    );
    true
}

/// Translate a raw `gilrs` event into a normalized [`PadInput`] (Xbox
/// layout), or `None` for events we don't map. Axis jitter near center is
/// dropped via a small deadzone. The mapping mirrors the keyboard model.
fn translate_pad(ev: gilrs::EventType) -> Option<PadInput> {
    use gilrs::{Axis, Button, EventType};
    use PadInput::*;
    match ev {
        EventType::ButtonPressed(b, _) => match b {
            Button::South => Some(FaceA),             // A
            Button::East => Some(FaceB),              // B
            Button::West => Some(FaceX),              // X
            Button::North => Some(FaceY),             // Y
            Button::LeftTrigger => Some(FocusDeckA),  // LB
            Button::RightTrigger => Some(FocusDeckB), // RB
            Button::LeftTrigger2 => Some(FadeA),      // LT
            Button::RightTrigger2 => Some(FadeB),     // RT
            Button::Start => Some(Quit),
            Button::Select => Some(Help),
            Button::DPadUp => Some(DpadUp),
            Button::DPadDown => Some(DpadDown),
            Button::DPadLeft => Some(DpadLeft),
            Button::DPadRight => Some(DpadRight),
            _ => None,
        },
        EventType::AxisChanged(axis, v, _) if v.abs() >= 0.1 => match axis {
            Axis::RightStickX => Some(Crossfade(v)), // continuous crossfade
            Axis::LeftStickX => Some(Jog(v)),        // jog/scratch focused deck
            _ => None,
        },
        _ => None,
    }
}

/// The `o` quick-demo track path: `TERMKRUSH_DEMO_TRACK` if set, else the
/// bundled fixture. Handy when the crate is empty.
fn demo_track_path() -> PathBuf {
    std::env::var("TERMKRUSH_DEMO_TRACK")
        .unwrap_or_else(|_| "tests/fixtures/sine_a440_10s.wav".to_string())
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn render(app: &App, w: u16, h: u16) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn splash_renders_wordmark_at_80x24() {
        let buf = render(&App::new(), 80, 24);
        let text = buffer_text(&buf);
        assert!(text.contains("TermKrush"), "wordmark missing:\n{text}");
        assert!(text.contains("? help"), "tagline missing:\n{text}");
    }

    #[test]
    fn wordmark_uses_amber_on_dark() {
        let buf = render(&App::new(), 80, 24);
        // Find the first cell of "TermKrush" and check its palette.
        let mut found = None;
        'outer: for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                if buf[(x, y)].symbol() == "T" {
                    found = Some((x, y));
                    break 'outer;
                }
            }
        }
        let (x, y) = found.expect("found the wordmark 'T'");
        let cell = &buf[(x, y)];
        assert_eq!(cell.fg, AMBER, "wordmark should be amber");
        assert_eq!(cell.bg, BG, "background should be CRT dark");
    }

    fn esc(app: &mut App) -> Action {
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
    }

    #[test]
    fn q_opens_quit_modal_then_y_confirms() {
        let mut app = App::new();
        // `q` no longer quits outright — it opens the confirm modal.
        assert_eq!(app.on_key(key('q')), Action::ConfirmQuit);
        assert!(
            app.confirm_quit && !app.should_quit,
            "modal open, not quitting yet"
        );
        // `y` confirms.
        assert_eq!(app.on_key(key('y')), Action::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn esc_opens_quit_modal_then_y_confirms() {
        let mut app = App::new();
        assert_eq!(esc(&mut app), Action::ConfirmQuit);
        assert!(app.confirm_quit && !app.should_quit);
        assert_eq!(app.on_key(key('y')), Action::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn quit_modal_cancels_with_n_or_esc() {
        let mut app = App::new();
        esc(&mut app);
        assert_eq!(app.on_key(key('n')), Action::ConfirmQuit);
        assert!(!app.confirm_quit && !app.should_quit, "n cancels");
        esc(&mut app); // reopen
        assert!(app.confirm_quit);
        esc(&mut app); // esc cancels
        assert!(!app.confirm_quit && !app.should_quit, "esc cancels");
    }

    #[test]
    fn quit_modal_swallows_other_keys() {
        let mut app = loaded_app(); // deck A loaded, not playing
        esc(&mut app); // open modal
        assert_eq!(
            app.on_key(key('j')),
            Action::None,
            "key behind modal is swallowed"
        );
        assert_ne!(
            app.mixer.deck(0).state(),
            DeckState::Playing,
            "deck untouched"
        );
        assert!(app.confirm_quit, "modal still open");
    }

    #[test]
    fn ctrl_c_quits_even_from_the_modal() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let mut app = App::new();
        assert_eq!(app.on_key(ctrl_c), Action::Quit);
        assert!(app.should_quit);
        // Also from inside the modal.
        let mut app2 = App::new();
        esc(&mut app2);
        assert_eq!(app2.on_key(ctrl_c), Action::Quit);
        assert!(app2.should_quit);
    }

    #[test]
    fn quit_modal_renders() {
        let mut app = App::new();
        esc(&mut app);
        assert!(buffer_text(&render(&app, 80, 24)).contains("Quit TermKrush?"));
    }

    #[test]
    fn question_mark_toggles_help_overlay() {
        let mut app = App::new();
        assert!(!app.show_help);
        assert_eq!(app.on_key(key('?')), Action::ToggleHelp);
        assert!(app.show_help);
        // The overlay actually renders its title.
        let buf = render(&app, 80, 24);
        assert!(buffer_text(&buf).contains("Help"));
        // Esc closes it.
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Action::ToggleHelp
        );
        assert!(!app.show_help);
    }

    #[test]
    fn unmapped_key_is_noop() {
        let mut app = App::new();
        assert_eq!(app.on_key(key('x')), Action::None);
        assert!(!app.should_quit);
    }

    /// An app whose deck has a short synthetic track loaded.
    fn loaded_app() -> App {
        use crate::audio::DecodedAudio;
        let mut app = App::new();
        app.focused_mut().load(DecodedAudio {
            samples: vec![0.5; 200],
            sample_rate: 44_100,
            channels: 2,
            source_sample_rate: 44_100,
            source_channels: 2,
            duration_secs: 100.0 / 44_100.0,
            title: Some("demo".into()),
            artist: None,
        });
        app
    }

    #[test]
    fn j_toggles_focused_deck_play_pause() {
        let mut app = loaded_app(); // track on deck A, focused by default
        assert_eq!(app.mixer.deck(0).state(), DeckState::Loaded);
        assert_eq!(app.on_key(key('j')), Action::PlayPause);
        assert_eq!(app.mixer.deck(0).state(), DeckState::Playing);
        assert_eq!(app.on_key(key('j')), Action::PlayPause);
        assert_eq!(app.mixer.deck(0).state(), DeckState::Paused);
    }

    #[test]
    fn k_stops_focused_deck() {
        let mut app = loaded_app();
        app.on_key(key('j')); // play
        assert_eq!(app.on_key(key('k')), Action::Stop);
        assert_eq!(app.mixer.deck(0).state(), DeckState::Stopped);
    }

    #[test]
    fn backslash_signals_open_without_doing_io() {
        let mut app = App::new();
        assert_eq!(app.on_key(key('\\')), Action::OpenFile);
        // on_key must not load anything itself — that's the event loop's job.
        assert_eq!(app.mixer.deck(0).state(), DeckState::Empty);
    }

    /// An app with a track of `frames` stereo frames at sample rate `rate`
    /// loaded (no ID3 title, so the file-name fallback is exercised). A low
    /// `rate` lets a few frames stand for whole seconds in the clock.
    fn app_with_track(frames: usize, rate: u32) -> App {
        use crate::audio::DecodedAudio;
        let mut app = App::new();
        app.focused_mut().load_named(
            DecodedAudio {
                samples: vec![0.4; frames * 2],
                sample_rate: rate,
                channels: 2,
                source_sample_rate: rate,
                source_channels: 2,
                duration_secs: frames as f64 / rate as f64,
                title: None,
                artist: None,
            },
            "sine_a440_10s.wav",
        );
        app
    }

    #[test]
    fn empty_deck_panel_shows_no_track() {
        let text = buffer_text(&render(&App::new(), 80, 24));
        assert!(text.contains("Deck A"), "panel title missing:\n{text}");
        assert!(text.contains("no track"), "empty prompt missing:\n{text}");
    }

    #[test]
    fn panel_shows_title_and_total_time_on_load() {
        // Loading updates the title (filename fallback) and total time.
        // Rendered at 100x30 (the design size) so the platter + readout fit.
        let app = app_with_track(1000, 100); // 10.0s
        let text = buffer_text(&render(&app, 100, 30));
        assert!(text.contains("sine_a440_10s.wav"), "title missing:\n{text}");
        assert!(text.contains("00:10.0"), "total time missing:\n{text}");
    }

    #[test]
    fn panel_elapsed_advances_then_freezes_and_glyph_changes() {
        let mut app = app_with_track(1000, 100); // 10.0s, rate 100 on deck A
        app.on_key(key('j')); // play A (focused deck)

        // Advance 3 seconds (300 frames at rate 100).
        let mut buf = vec![0.0f32; 600];
        app.focused_mut().fill(&mut buf);
        let text = buffer_text(&render(&app, 80, 24));
        assert!(
            text.contains("00:03.0"),
            "elapsed should tick to 3s:\n{text}"
        );
        assert!(text.contains('▶'), "playing glyph missing:\n{text}");

        // Pause: elapsed freezes and the glyph changes.
        app.on_key(key('j'));
        let before = app.focused_mut().position_secs();
        app.focused_mut().fill(&mut vec![0.0f32; 600]); // no-op while paused
        assert_eq!(
            app.focused_mut().position_secs(),
            before,
            "paused elapsed frozen"
        );
        let text = buffer_text(&render(&app, 80, 24));
        assert!(text.contains('⏸'), "paused glyph missing:\n{text}");
    }

    #[test]
    fn progress_bar_fills_proportionally() {
        assert_eq!(progress_bar(0.0, 10), "[░░░░░░░░░░]");
        assert_eq!(progress_bar(1.0, 10), "[██████████]");
        assert_eq!(progress_bar(0.5, 10), "[█████░░░░░]");
        // Out-of-range input is clamped.
        assert_eq!(progress_bar(2.0, 4), "[████]");
        assert_eq!(progress_bar(-1.0, 4), "[░░░░]");
    }

    #[test]
    fn transport_glyph_per_state() {
        assert_eq!(transport_glyph(DeckState::Playing), "▶");
        assert_eq!(transport_glyph(DeckState::Paused), "⏸");
        assert_eq!(transport_glyph(DeckState::Stopped), "■");
        assert_eq!(transport_glyph(DeckState::Loaded), "⏏");
        assert_eq!(transport_glyph(DeckState::Empty), "·");
    }

    #[test]
    fn clock_formats_mm_ss() {
        assert_eq!(fmt_clock(0.0), "00:00.0");
        assert_eq!(fmt_clock(9.25), "00:09.2");
        assert_eq!(fmt_clock(75.0), "01:15.0");
    }

    #[test]
    fn w_s_nudge_focused_deck_gain_tab_switches_target() {
        let mut app = loaded_app(); // deck A focused
        assert_eq!(app.on_key(key('w')), Action::DeckGain); // A up
        assert!((app.mixer.deck(0).gain() - 1.05).abs() < 1e-6);
        assert_eq!(app.on_key(key('s')), Action::DeckGain); // A down
        assert!((app.mixer.deck(0).gain() - 1.0).abs() < 1e-6);
        // Focus deck B; now w/s drive B's gain, leaving A alone.
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.on_key(key('w')), Action::DeckGain); // B up
        assert!((app.mixer.deck(1).gain() - 1.05).abs() < 1e-6);
        assert!(
            (app.mixer.deck(0).gain() - 1.0).abs() < 1e-6,
            "deck A untouched once B is focused"
        );
    }

    #[test]
    fn brackets_nudge_master_gain() {
        let mut app = loaded_app();
        assert_eq!(app.on_key(key(']')), Action::MasterGain);
        assert!((app.mixer.master_gain() - 1.05).abs() < 1e-6);
        assert_eq!(app.on_key(key('[')), Action::MasterGain);
        assert!((app.mixer.master_gain() - 1.0).abs() < 1e-6);
        // Deck gain is untouched by master keys.
        assert!((app.mixer.deck(0).gain() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn panel_shows_db_readout_and_master() {
        let app = app_with_track(1000, 100); // gain 1.0, master 1.0
        let text = buffer_text(&render(&app, 100, 36));
        assert!(
            text.contains("+0.0 dB"),
            "unity dB readout missing:\n{text}"
        );
        assert!(text.contains("master"), "master readout missing:\n{text}");
    }

    #[test]
    fn fmt_db_values() {
        assert_eq!(fmt_db(1.0), "+0.0 dB");
        assert_eq!(fmt_db(0.5), "-6.0 dB");
        assert_eq!(fmt_db(0.0), "-inf dB");
    }

    #[test]
    fn a_d_jog_focused_deck_fine_and_shift_coarse() {
        let mut app = app_with_track(2000, 100); // 20s on focused deck A
                                                 // Fine jog: d forward, a back (SEEK_SCRUB = 0.1s).
        assert_eq!(app.on_key(key('d')), Action::Seek);
        assert!(
            (app.mixer.deck(0).position_secs() - 0.1).abs() < 1e-9,
            "d => +0.1s"
        );
        assert_eq!(app.on_key(key('a')), Action::Seek);
        assert!(
            app.mixer.deck(0).position_secs().abs() < 1e-9,
            "a => back to 0"
        );
        app.on_key(key('a'));
        assert_eq!(app.mixer.deck(0).position_frames(), 0, "clamps at start");
        // Coarse jog: Shift+d = far seek past the 20s end => clamp to EOF + stop.
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::SHIFT)),
            Action::Seek
        );
        assert_eq!(app.mixer.deck(0).position_frames(), 2000, "clamped to EOF");
        assert_eq!(app.mixer.deck(0).state(), DeckState::Stopped);
    }

    /// An app with a crate of the given file names loaded.
    fn app_with_crate(names: &[&str]) -> App {
        use crate::library::{Crate, CrateEntry};
        let entries = names
            .iter()
            .map(|n| CrateEntry {
                path: PathBuf::from(format!("/music/{n}")),
                name: n.to_string(),
            })
            .collect();
        let mut app = App::new();
        app.set_crate(Crate::from_entries(entries));
        app
    }

    #[test]
    fn slash_opens_filter_and_typing_narrows() {
        let mut app = app_with_crate(&["alpha.mp3", "beta.mp3", "gamma.mp3"]);
        assert_eq!(app.on_key(key('/')), Action::Filter);
        assert_eq!(app.on_key(key('b')), Action::Filter);
        let vis = app.visible();
        assert_eq!(vis.len(), 1);
        assert_eq!(vis[0].name, "beta.mp3");
        // Esc clears the filter and closes it.
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Action::Filter
        );
        assert!(app.filter.is_none());
        assert_eq!(app.visible().len(), 3);
    }

    #[test]
    fn arrows_browse_the_focused_crate_and_enter_loads() {
        let mut app = app_with_crate(&["alpha.mp3", "beta.mp3", "gamma.mp3"]);
        let down = || KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up = || KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let left = || KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        // Default focus is Deck A; arrow left to focus the crate.
        assert_eq!(app.on_key(left()), Action::Focus);
        assert_eq!(app.focus_cell(), Focus::Crate);
        assert_eq!(app.on_key(down()), Action::CrateNav); // -> beta
        assert_eq!(app.on_key(down()), Action::CrateNav); // -> gamma
        assert_eq!(app.on_key(up()), Action::CrateNav); // -> beta
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Action::LoadSelected
        );
        assert_eq!(
            app.take_pending_load(),
            Some(PathBuf::from("/music/beta.mp3"))
        );
    }

    #[test]
    fn crate_list_clamps_at_ends_when_focused() {
        let mut app = app_with_crate(&["a.mp3", "b.mp3"]);
        let down = || KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up = || KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)); // focus crate
        app.on_key(up()); // already at top, stays 0
        assert_eq!(app.crate_sel, 0);
        app.on_key(down());
        app.on_key(down());
        app.on_key(down()); // past the end, clamps to last
        assert_eq!(app.crate_sel, 1);
    }

    #[test]
    fn enter_on_empty_crate_is_noop() {
        let mut app = App::new(); // empty crate
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Action::None
        );
        assert!(app.take_pending_load().is_none());
    }

    #[test]
    fn filter_enter_loads_highlight_and_closes() {
        let mut app = app_with_crate(&["alpha.mp3", "beta.mp3"]);
        app.on_key(key('/'));
        app.on_key(key('a'));
        app.on_key(key('l')); // "al" subsequence -> only alpha
        assert_eq!(app.visible().len(), 1);
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Action::LoadSelected
        );
        assert!(app.filter.is_none(), "filter closes after load");
        assert_eq!(
            app.take_pending_load(),
            Some(PathBuf::from("/music/alpha.mp3"))
        );
    }

    #[test]
    fn crate_panel_renders_names_and_filter_title() {
        let app = app_with_crate(&["alpha.mp3", "beta.mp3"]);
        let text = buffer_text(&render(&app, 80, 24));
        assert!(text.contains("alpha.mp3"), "track name missing:\n{text}");
        assert!(text.contains("Crate"), "crate title missing:\n{text}");

        let mut app2 = app_with_crate(&["alpha.mp3", "beta.mp3"]);
        app2.on_key(key('/'));
        app2.on_key(key('b'));
        let text2 = buffer_text(&render(&app2, 80, 24));
        assert!(text2.contains("/b"), "filter query not in title:\n{text2}");
    }

    /// A constant-level stereo track of `frames` stereo frames.
    fn synth_track(frames: usize) -> crate::audio::DecodedAudio {
        crate::audio::DecodedAudio {
            samples: vec![0.5; frames * 2],
            sample_rate: 44_100,
            channels: 2,
            source_sample_rate: 44_100,
            source_channels: 2,
            duration_secs: frames as f64 / 44_100.0,
            title: None,
            artist: None,
        }
    }

    #[test]
    fn tab_steps_through_every_cell_and_wraps() {
        let mut app = App::new();
        let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus_cell(), Focus::Deck(0));
        // The order: decks, mixer cells, pads, DJ, crate, …
        for want in [
            Focus::Deck(1),
            Focus::MixSoft,
            Focus::MixHard,
            Focus::Pad(0),
        ] {
            assert_eq!(app.on_key(tab), Action::Focus);
            assert_eq!(app.focus_cell(), want);
        }
        // 13 cells total (crate + 2 decks + 2 mixer + 7 pads + DJ): a full
        // cycle returns to the start.
        let start = app.focus_cell();
        for _ in 0..13 {
            app.on_key(tab);
        }
        assert_eq!(app.focus_cell(), start, "Tab wraps after every cell");
    }

    #[test]
    fn action_cluster_follows_the_focused_deck() {
        let mut app = App::new();
        app.mixer.deck_mut(0).load(synth_track(2000));
        app.mixer.deck_mut(1).load(synth_track(2000));

        // Deck A focused: `j` plays A only.
        app.on_key(key('j'));
        assert_eq!(app.mixer.deck(0).state(), DeckState::Playing);
        assert_eq!(app.mixer.deck(1).state(), DeckState::Loaded);

        // Tab to deck B; the same `j` now drives B — both play.
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.on_key(key('j'));
        assert_eq!(
            app.mixer.deck(0).state(),
            DeckState::Playing,
            "A keeps playing"
        );
        assert_eq!(app.mixer.deck(1).state(), DeckState::Playing);

        // `k` stops the focused deck (B); A is undisturbed.
        app.on_key(key('k'));
        assert_eq!(app.mixer.deck(1).state(), DeckState::Stopped);
        assert_eq!(
            app.mixer.deck(0).state(),
            DeckState::Playing,
            "A undisturbed"
        );
    }

    #[test]
    fn both_deck_panels_and_bars_visible_at_100x30() {
        // Acceptance: both decks fully visible with position bars at 100x30.
        let app = App::new();
        let text = buffer_text(&render(&app, 100, 30));
        assert!(text.contains("Deck A"), "Deck A panel missing:\n{text}");
        assert!(text.contains("Deck B"), "Deck B panel missing:\n{text}");
        assert!(
            text.contains("▸ Deck A"),
            "focus marker should be on Deck A by default:\n{text}"
        );
        // Position bars render (empty decks -> all-empty bar).
        assert!(text.contains('['), "deck position bars missing:\n{text}");
        // Mixer row shows the deck-blend / auto-fade state.
        assert!(
            text.contains("auto-fade"),
            "transition readout missing:\n{text}"
        );
    }

    #[test]
    fn deck_panel_shows_bpm_once_detected() {
        let mut app = app_with_track(1000, 100);
        app.mixer.deck_mut(0).set_bpm(128.0); // simulate background detection
        let text = buffer_text(&render(&app, 100, 30));
        assert!(
            text.contains("128 BPM"),
            "deck panel should show the detected BPM:\n{text}"
        );
    }

    #[test]
    fn comma_period_varispeed_the_focused_deck() {
        let mut app = app_with_track(2000, 100); // deck A focused
        app.mixer.deck_mut(0).set_bpm(120.0); // detected base
                                              // `.` speeds up 1% → effective BPM = 120 × 1.01 = 121.2.
        assert_eq!(app.on_key(key('.')), Action::Bpm);
        assert!((app.mixer.deck(0).speed() - 1.01).abs() < 1e-6);
        assert!((app.mixer.deck(0).bpm().unwrap() - 121.2).abs() < 1e-3);
        // shift = fine (0.1%); `,` slows.
        app.on_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::SHIFT));
        assert!((app.mixer.deck(0).speed() - 1.009).abs() < 1e-6);
        // Detection updating the base flows through (effective tracks speed).
        app.mixer.deck_mut(0).set_bpm(100.0);
        assert!((app.mixer.deck(0).bpm().unwrap() - 100.9).abs() < 1e-3);
    }

    #[test]
    fn mark_in_out_records_a_clip_off_the_deck() {
        use crate::audio::DecodedAudio;
        use crate::test_support::{flat_stereo, frames};
        let rate = 1000;
        let mut app = App::new(); // Deck A focused by default
        app.mixer.deck_mut(0).load_named(
            DecodedAudio {
                samples: flat_stereo(1000, 0.5),
                sample_rate: rate,
                channels: 2,
                source_sample_rate: rate,
                source_channels: 2,
                duration_secs: 1.0,
                title: None,
                artist: None,
            },
            "x.wav",
        );
        app.mixer.deck_mut(0).seek(0.1); // playhead → frame 100
        assert!(!app.recording(0));
        assert_eq!(app.on_key(key('l')), Action::Mark); // mark in
        assert!(app.recording(0));
        app.mixer.deck_mut(0).seek(0.3); // playhead → frame 300
        assert_eq!(app.on_key(key(';')), Action::Mark); // mark out → capture [100, 300)
        assert!(!app.recording(0), "disarmed after capture");
        assert_eq!(app.recordings().len(), 1);
        assert_eq!(frames(&app.recordings()[0].samples), 200);
        assert!(app.recordings()[0]
            .samples
            .iter()
            .all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn r_records_the_live_mix_into_the_stash() {
        let mut app = loaded_app(); // deck A loaded + focused
        app.mixer.deck_mut(0).play();
        assert_eq!(app.on_key(key('r')), Action::Record); // arm
        assert!(app.mixer.is_recording());
        app.mixer.fill_mix(&mut [0.0f32; 256]); // the pump captures a block
        assert_eq!(app.on_key(key('r')), Action::Record); // disarm → stashed
        assert!(!app.mixer.is_recording());
        // Deck A is focused (not a pad), so it lands in the recordings stash.
        assert_eq!(app.recordings().len(), 1);
        assert!(!app.recordings()[0].samples.is_empty());
    }

    #[test]
    fn mark_out_without_in_is_a_noop() {
        let mut app = loaded_app();
        assert_eq!(app.on_key(key(';')), Action::None);
        assert!(app.recordings().is_empty());
    }

    /// Move focus from Deck A down to Pad 0 (Deck A → MixSoft → Pad 0).
    fn focus_first_pad(app: &mut App) {
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        app.on_key(down);
        app.on_key(down);
        assert_eq!(app.focus_cell(), Focus::Pad(0));
    }

    #[test]
    fn arrows_walk_the_control_grid() {
        let mut app = App::new();
        let go = |app: &mut App, code| app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(app.focus_cell(), Focus::Deck(0));
        assert_eq!(go(&mut app, KeyCode::Right), Action::Focus);
        assert_eq!(app.focus_cell(), Focus::Deck(1)); // right column
        go(&mut app, KeyCode::Down);
        assert_eq!(app.focus_cell(), Focus::MixHard); // down a row
        go(&mut app, KeyCode::Left);
        assert_eq!(app.focus_cell(), Focus::MixSoft); // left column
        go(&mut app, KeyCode::Left);
        assert_eq!(app.focus_cell(), Focus::Crate); // off the left edge → crate
    }

    #[test]
    fn mixer_cells_dispatch_cut_and_fade() {
        let mut app = App::new();
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        app.on_key(down); // Deck A → MixSoft
        assert_eq!(app.focus_cell(), Focus::MixSoft);
        assert_eq!(app.on_key(key('k')), Action::Crossfade); // soft: fade to B
        assert!(app.mixer.is_fading());
        // Move to the hard-cut cell and hard-cut to A.
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.focus_cell(), Focus::MixHard);
        assert_eq!(app.on_key(key('j')), Action::Crossfade);
        assert_eq!(app.mixer.xfade_applied(), -1.0, "hard cut to A is instant");
    }

    #[test]
    fn pad_bumpers_focus_decks_and_face_buttons_act() {
        let mut app = loaded_app(); // deck A loaded + focused
        assert_eq!(app.on_pad(PadInput::FocusDeckB), Action::Focus);
        assert_eq!(app.focus_cell(), Focus::Deck(1));
        assert_eq!(app.on_pad(PadInput::FocusDeckA), Action::Focus);
        assert_eq!(app.focus_cell(), Focus::Deck(0));
        assert_eq!(app.on_pad(PadInput::FaceA), Action::PlayPause); // A = play
        assert_eq!(app.mixer.deck(0).state(), DeckState::Playing);
        assert_eq!(app.on_pad(PadInput::FaceB), Action::Stop); // B = cue/stop
    }

    #[test]
    fn pad_dpad_navigates_and_triggers_and_sticks_blend() {
        let mut app = App::new();
        assert_eq!(app.on_pad(PadInput::DpadRight), Action::Focus);
        assert_eq!(app.focus_cell(), Focus::Deck(1)); // dpad = grid nav
        assert_eq!(app.on_pad(PadInput::FadeB), Action::Crossfade); // RT auto-fade
        assert!(app.mixer.is_fading());
        // Right stick = continuous crossfade (instant blend position).
        assert_eq!(app.on_pad(PadInput::Crossfade(-1.0)), Action::Crossfade);
        assert_eq!(app.mixer.xfade_applied(), -1.0);
    }

    #[test]
    fn pad_start_opens_quit_modal_then_a_confirms() {
        let mut app = App::new();
        assert_eq!(app.on_pad(PadInput::Quit), Action::ConfirmQuit);
        assert!(app.confirm_quit && !app.should_quit);
        assert_eq!(app.on_pad(PadInput::FaceA), Action::Quit); // A = yes
        assert!(app.should_quit);
    }

    #[test]
    fn comma_period_nudge_focused_pad_bpm() {
        let mut app = App::new();
        focus_first_pad(&mut app);
        assert_eq!(app.on_key(key('.')), Action::Bpm);
        assert_eq!(app.mixer.pad_bpm(0), Some(121.0));
        // Deck A's BPM is untouched by pad nudges.
        assert_eq!(app.mixer.deck(0).bpm(), None);
    }

    #[test]
    fn deck_border_is_amber_focused_dim_unfocused() {
        // Acceptance: focus border colors match the design.
        assert_eq!(deck_border(true), AMBER);
        assert_eq!(deck_border(false), Color::DarkGray);
    }

    #[test]
    fn grid_renders_all_cells() {
        // The control grid shows decks, both mixer cells, all 7 pads, and DJ.
        let text = buffer_text(&render(&App::new(), 100, 36));
        for needle in [
            "Deck A",
            "Deck B",
            "Mix · soft",
            "Mix · hard",
            "Pad 1",
            "Pad 7",
            "DJ",
        ] {
            assert!(
                text.contains(needle),
                "grid cell {needle:?} missing:\n{text}"
            );
        }
    }

    #[test]
    fn z_collapses_and_expands_the_crate() {
        let mut app = app_with_crate(&["alpha.mp3"]);
        // Visible by default: the crate title shows.
        assert!(buffer_text(&render(&app, 100, 30)).contains("Crate"));
        // `z` collapses it.
        assert_eq!(app.on_key(key('z')), Action::ToggleCrate);
        assert!(
            !buffer_text(&render(&app, 100, 30)).contains("Crate"),
            "crate should be hidden when collapsed"
        );
        // `z` again brings it back.
        assert_eq!(app.on_key(key('z')), Action::ToggleCrate);
        assert!(buffer_text(&render(&app, 100, 30)).contains("Crate"));
    }

    #[test]
    fn g_h_hard_cut_instantly() {
        let mut app = App::new();
        assert_eq!(app.on_key(key('g')), Action::Crossfade);
        assert_eq!(
            app.mixer.xfade_applied(),
            -1.0,
            "g hard-cuts to A instantly"
        );
        assert!(!app.mixer.is_fading());
        assert_eq!(app.on_key(key('h')), Action::Crossfade);
        assert_eq!(app.mixer.xfade_applied(), 1.0, "h hard-cuts to B instantly");
    }

    #[test]
    fn shift_g_h_start_an_autofade() {
        let mut app = App::new();
        app.on_key(key('g')); // sit on A
        assert_eq!(app.on_key(key('H')), Action::Crossfade); // auto-fade to B
        assert_eq!(app.mixer.xfade(), 1.0, "target is B");
        assert!(app.mixer.is_fading(), "fade is in progress, not instant");
    }

    #[test]
    fn space_cycles_the_fade_duration() {
        let mut app = App::new();
        assert_eq!(app.fade_secs(), 1.0);
        assert_eq!(app.on_key(key(' ')), Action::Crossfade);
        assert_eq!(app.fade_secs(), 2.0);
        app.on_key(key(' '));
        app.on_key(key(' '));
        assert_eq!(app.fade_secs(), 8.0);
        app.on_key(key(' '));
        assert_eq!(app.fade_secs(), 1.0, "wraps back around");
    }

    #[test]
    fn ellipsize_truncates_long_with_marker() {
        assert_eq!(ellipsize("short", 10), "short");
        let e = ellipsize("a very long track name.mp3", 10);
        assert_eq!(e.chars().count(), 10);
        assert!(e.ends_with('…'));
        assert!(!e.contains("name"), "tail should be dropped: {e}");
    }

    #[test]
    fn empty_crate_shows_wrapped_help_not_truncated() {
        // The how-to wraps to fit the 32-col panel rather than running off
        // the edge — the key tokens survive in full.
        let text = buffer_text(&render(&App::new(), 100, 30));
        assert!(
            text.contains("crate_root"),
            "missing crate_root hint:\n{text}"
        );
        assert!(
            text.contains("config.toml"),
            "config hint got cut off:\n{text}"
        );
    }

    #[test]
    fn long_track_names_are_ellipsized_in_the_crate() {
        let app = app_with_crate(&["This Is An Absurdly Long Track Title That Overflows.mp3"]);
        let text = buffer_text(&render(&app, 100, 30));
        assert!(
            text.contains('…'),
            "long name should be ellipsized:\n{text}"
        );
    }

    #[test]
    fn long_deck_track_name_is_ellipsized() {
        use crate::audio::DecodedAudio;
        let mut app = App::new();
        app.mixer.deck_mut(0).load_named(
            DecodedAudio {
                samples: vec![0.1; 4],
                sample_rate: 44_100,
                channels: 2,
                source_sample_rate: 44_100,
                source_channels: 2,
                duration_secs: 0.0,
                title: None,
                artist: None,
            },
            "An Absurdly Long Track Title That Will Not Fit In The Deck Panel.mp3",
        );
        let text = buffer_text(&render(&app, 100, 30));
        assert!(text.contains('…'), "deck name should ellipsize:\n{text}");
        assert!(
            !text.contains("Not Fit In The Deck"),
            "the full long name should not render:\n{text}"
        );
    }

    #[test]
    fn mixer_panel_shows_transition_state_and_fade_duration() {
        let mut app = App::new();
        // Default: both decks live, 1s fade.
        let text = buffer_text(&render(&app, 100, 30));
        assert!(text.contains("A + B"), "blend state missing:\n{text}");
        assert!(
            text.contains("auto-fade 1s"),
            "fade duration missing:\n{text}"
        );
        // Hard cut to B → shows ▶ B.
        app.on_key(key('h'));
        assert!(buffer_text(&render(&app, 100, 30)).contains("▶ B"));
    }

    #[test]
    fn note_loaded_dedups_caps_and_orders() {
        let mut app = App::new();
        for i in 0..8 {
            app.note_loaded(PathBuf::from(format!("/m/{i}.mp3")));
        }
        assert_eq!(app.recent().len(), RECENT_CAP, "capped at {RECENT_CAP}");
        assert_eq!(
            app.recent()[0],
            PathBuf::from("/m/7.mp3"),
            "most recent first"
        );
        // Re-loading an existing track moves it to the front, no growth/dupes.
        let again = PathBuf::from("/m/5.mp3");
        app.note_loaded(again.clone());
        assert_eq!(app.recent()[0], again);
        assert_eq!(app.recent().len(), RECENT_CAP);
        let dupes = app.recent().iter().filter(|p| **p == again).count();
        assert_eq!(dupes, 1, "no duplicates in the shortlist");
    }

    #[test]
    fn loaded_panel_lists_recent_tracks() {
        let mut app = app_with_crate(&["x.mp3"]);
        assert!(buffer_text(&render(&app, 100, 30)).contains("(none yet)"));
        app.note_loaded(PathBuf::from("/music/banger.mp3"));
        let text = buffer_text(&render(&app, 100, 30));
        assert!(
            text.contains("Loaded"),
            "loaded panel title missing:\n{text}"
        );
        assert!(text.contains("banger.mp3"), "recent track missing:\n{text}");
    }

    // ---- end-to-end command flow (the gap that let "enter does nothing" ship) ----

    /// A real, decodable fixture on disk (the committed CC0 sine WAV).
    fn fixture_wav() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine_a440_10s.wav")
    }

    /// An app whose crate holds one real fixture track (so a load actually
    /// decodes a file end-to-end).
    fn app_with_real_crate() -> App {
        use crate::library::{Crate, CrateEntry};
        let p = fixture_wav();
        let name = p.file_name().unwrap().to_str().unwrap().to_string();
        let mut app = App::new();
        app.set_crate(Crate::from_entries(vec![CrateEntry { name, path: p }]));
        app
    }

    /// Drive a load/assign action to completion: kick off the background
    /// decode, wait for the result, and apply it — the async path the event
    /// loop runs, collapsed to synchronous for the test.
    fn drive_action(app: &mut App, action: Action) {
        let (tx, rx) = std::sync::mpsc::channel();
        let kicked = apply_load_action(app, action, 44_100, &tx)
            || apply_pad_assign(app, action, 44_100, &tx);
        assert!(kicked, "action should kick off a background decode");
        drop(tx); // leave only the decode thread's sender
        if let Ok(d) = rx.recv() {
            app.place_decoded(d);
        }
    }

    #[test]
    fn enter_loads_the_selected_track_end_to_end() {
        let mut app = app_with_real_crate();
        let act = app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(act, Action::LoadSelected, "enter signals a load");
        // Decode (off-thread) + land on the deck — the step old tests skipped.
        drive_action(&mut app, act);
        assert_eq!(app.mixer.deck(0).state(), DeckState::Loaded);
        assert!(app
            .mixer
            .deck(0)
            .display_name()
            .unwrap_or("")
            .contains("sine_a440"));
        assert!((app.mixer.deck(0).duration_secs() - 10.0).abs() < 0.1);
        assert!(!app.recent().is_empty(), "load lands in the shortlist");
    }

    #[test]
    fn enter_is_non_blocking_sets_loading_then_clears_it() {
        // The freeze fix: enter must not decode on the calling thread — it
        // flags the deck as loading and returns immediately.
        let mut app = app_with_real_crate();
        let (tx, rx) = std::sync::mpsc::channel();
        let act = app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(apply_load_action(&mut app, act, 44_100, &tx));
        assert!(
            app.is_loading(0),
            "deck shows loading while decoding off-thread"
        );
        assert_eq!(
            app.mixer.deck(0).state(),
            DeckState::Empty,
            "not loaded synchronously"
        );
        drop(tx);
        app.place_decoded(rx.recv().unwrap());
        assert!(!app.is_loading(0), "loading clears once the decode lands");
        assert_eq!(app.mixer.deck(0).state(), DeckState::Loaded);
    }

    #[test]
    fn cached_bpm_is_applied_on_load() {
        let mut app = app_with_real_crate();
        let path = app.selected_path().unwrap();
        app.bpm_cache.insert(path, 130.0); // pre-seed → detection skipped
        let act = app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        drive_action(&mut app, act);
        assert_eq!(
            app.mixer.deck(0).bpm(),
            Some(130.0),
            "cached BPM applied on load"
        );
    }

    #[test]
    fn loaded_track_produces_audible_output_when_played() {
        // The whole chain: select → enter → (bg decode) → load → play → mix.
        let mut app = app_with_real_crate();
        let act = app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        drive_action(&mut app, act);
        assert_eq!(app.on_key(key('j')), Action::PlayPause); // play focused deck
        let mut buf = vec![0.0f32; 4096];
        app.mixer.fill_mix(&mut buf);
        assert!(
            buf.iter().any(|&s| s.abs() > 0.01),
            "playing a loaded track should produce non-silent audio"
        );
    }

    #[test]
    fn every_command_key_maps_to_an_action() {
        // Transport + mixer keys (don't change input mode).
        let mut a = loaded_app();
        let cases = [
            ('j', Action::PlayPause), // focused-deck action cluster
            ('k', Action::Stop),
            ('w', Action::DeckGain), // focused-deck value
            ('s', Action::DeckGain),
            ('a', Action::Seek), // jog
            ('d', Action::Seek),
            ('g', Action::Crossfade),
            ('h', Action::Crossfade),
            ('G', Action::Crossfade),
            ('H', Action::Crossfade),
            (' ', Action::Crossfade),
            ('[', Action::MasterGain),
            (']', Action::MasterGain),
            (',', Action::Bpm),
            ('.', Action::Bpm),
            ('z', Action::ToggleCrate),
            ('\\', Action::OpenFile),
            ('1', Action::TriggerPad),
            ('4', Action::TriggerPad),
        ];
        for (k, want) in cases {
            assert_eq!(a.on_key(key(k)), want, "key {k:?} unmapped/wrong");
        }
        assert_eq!(
            a.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Action::Focus
        );
        assert_eq!(a.on_key(key('?')), Action::ToggleHelp);
        assert_eq!(
            a.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Action::ToggleHelp
        );

        // Crate keys on a populated crate. Arrow left focuses the crate, then
        // up/down browse it; enter loads; `/` enters filter mode.
        let mut c = app_with_crate(&["a.mp3", "b.mp3"]);
        assert_eq!(
            c.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Action::Focus
        );
        assert_eq!(
            c.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Action::CrateNav
        );
        assert_eq!(
            c.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            Action::CrateNav
        );
        assert_eq!(
            c.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Action::LoadSelected
        );
        assert_eq!(c.on_key(key('/')), Action::Filter);

        assert_eq!(a.on_key(key('q')), Action::ConfirmQuit); // q opens the quit modal
    }

    #[test]
    fn number_keys_trigger_sampler_pads() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 32]);
        assert_eq!(app.on_key(key('1')), Action::TriggerPad);
        assert_eq!(app.mixer.active_voices(), 1, "pad 1 triggered a voice");
        // An empty pad still maps but produces no voice.
        assert_eq!(app.on_key(key('2')), Action::TriggerPad);
        assert_eq!(app.mixer.active_voices(), 1);
    }

    #[test]
    fn focused_pad_triggers_with_j() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 32]); // a clip in slot 0
        focus_first_pad(&mut app); // arrow down to Pad 0
        assert_eq!(app.on_key(key('j')), Action::TriggerPad);
        assert_eq!(app.mixer.active_voices(), 1);
        // Arrow right moves to Pad 1 (the other column).
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.focus_cell(), Focus::Pad(1));
    }

    #[test]
    fn b_toggles_auto_bpm_on_the_focused_pad() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        focus_first_pad(&mut app);
        assert!(!app.mixer.pad_autobpm(0));
        assert_eq!(app.on_key(key('b')), Action::Mark);
        assert!(app.mixer.pad_autobpm(0));
    }

    #[test]
    fn focused_pad_semicolon_cycles_the_pattern() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        focus_first_pad(&mut app);
        assert_eq!(app.mixer.pad_pattern(0), Pattern::Straight);
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Char(';'), KeyModifiers::NONE)),
            Action::Mark
        );
        assert_eq!(app.mixer.pad_pattern(0), Pattern::Cut);
    }

    #[test]
    fn focused_pad_a_d_w_s_trim_the_clip_nondestructively() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 20_000]); // 10_000 frames
        focus_first_pad(&mut app); // Pad 0
        let (in0, out0) = app.mixer.pad_trim(0);
        assert_eq!((in0, out0), (0, 10_000));
        assert_eq!(app.on_key(key('d')), Action::Mark); // in-point forward
        assert!(app.mixer.pad_trim(0).0 > 0);
        assert_eq!(app.on_key(key('s')), Action::Mark); // out-point in
        assert!(app.mixer.pad_trim(0).1 < 10_000);
        // Non-destructive: the clip is still its full length.
        assert_eq!(app.mixer.pad_clip_frames(0), 10_000);
    }

    #[test]
    fn focused_pad_k_assigns_latest_recording_then_triggers() {
        let mut app = App::new();
        app.recordings.push(Clip::new(
            crate::test_support::flat_stereo(64, 0.5),
            Some(126.0),
            "rec",
        ));
        focus_first_pad(&mut app); // Pad 0
        assert_eq!(app.on_key(key('k')), Action::AssignPad);
        assert!(app.mixer.pad_loaded(0), "recording landed on the pad");
        assert_eq!(app.mixer.pad_bpm(0), Some(126.0), "clip BPM carried over");
        // And it triggers like any pad clip.
        assert_eq!(app.on_key(key('j')), Action::TriggerPad);
        assert_eq!(app.mixer.active_voices(), 1);
    }

    #[test]
    fn focused_pad_l_assigns_selected_crate_track() {
        let mut app = app_with_real_crate(); // crate holds the real WAV fixture
        focus_first_pad(&mut app);
        let act = app.on_key(key('l')); // assign highlighted crate track to Pad 0
        assert_eq!(act, Action::AssignPad);
        drive_action(&mut app, act); // decodes off-thread, then assigns
        assert!(app.mixer.pad_loaded(0), "slot 0 now holds a clip");
    }

    #[test]
    fn dj_cat_bobs_on_the_beat_and_idles() {
        use crate::audio::DecodedAudio;
        use crate::test_support::flat_stereo;
        let rate = 100;
        let mut app = App::new();
        // Idle (nothing playing): rest frame, and the cat face renders.
        assert_eq!(dj_frame(&app), 0);
        assert!(
            buffer_text(&render(&app, 100, 36)).contains("=^"),
            "cat face renders"
        );

        app.mixer.deck_mut(0).load(DecodedAudio {
            samples: flat_stereo(400, 0.4),
            sample_rate: rate,
            channels: 2,
            source_sample_rate: rate,
            source_channels: 2,
            duration_secs: 4.0,
            title: None,
            artist: None,
        });
        app.mixer.deck_mut(0).set_bpm(120.0);
        app.mixer.deck_mut(0).play();
        assert_eq!(dj_frame(&app), 0, "rest on the downbeat");
        // 0.5s = one beat at 120 BPM → the bob frame flips.
        app.mixer.deck_mut(0).fill(&mut vec![0.0f32; 100]); // 50 frames = 0.5s
        assert_eq!(dj_frame(&app), 1, "bobbed after one beat");
    }

    #[test]
    fn pad_cell_reflects_assignment() {
        let mut app = App::new();
        let text = buffer_text(&render(&app, 100, 36));
        assert!(text.contains("Pad 1"), "pad 1 cell missing:\n{text}");
        assert!(text.contains('·'), "empty pad marker missing:\n{text}");
        app.mixer.assign_pad(0, vec![0.5; 8]);
        let text = buffer_text(&render(&app, 100, 36));
        assert!(text.contains('●'), "assigned pad marker missing:\n{text}");
    }
}
