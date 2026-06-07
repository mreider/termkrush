//! Terminal user interface (ratatui + crossterm) — pads-only shell.
//!
//! The split is deliberate so the UI is testable without a tty:
//!
//! - [`App`] holds UI state and maps input events to [`Action`]s — pure,
//!   unit-tested directly.
//! - [`draw`] renders the current state into a ratatui [`Frame`] — pure,
//!   tested headlessly via `TestBackend`.
//! - [`run`] owns the messy parts: alternate screen, raw mode, the redraw
//!   loop, and (via [`TerminalGuard`]) restoring the terminal on the way out.
//!
//! This is the thin shell over `termkrush`'s engine (the mixer/pads); it
//! imports the engine but holds only presentation + input-mapping state.
//! Colors follow the CRT palette: amber wordmark, green tagline, near-black
//! background.

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
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use termkrush_core::audio::{AudioOutput, DecodedAudio};
use termkrush_core::clip::Clip;
use termkrush_core::config::Config;
use termkrush_core::library::Crate;
use termkrush_core::mix::{Mixer, PadKind, PADS};
use termkrush_core::scratch::ScratchUnit;
use termkrush_core::timeline::Timeline;

/// Per-keypress master-gain nudge (linear).
const GAIN_NUDGE: f32 = 0.05;
/// Trim nudge in frames: ~0.1 s coarse, ~0.01 s fine (44.1k-ish).
const TRIM_COARSE: i64 = 4410;
const TRIM_FINE: i64 = 441;

/// CRT amber, `#ffb000` — the wordmark and accents.
pub const AMBER: Color = Color::Rgb(0xff, 0xb0, 0x00);
/// CRT green, `#45f07d` — secondary text.
pub const GREEN: Color = Color::Rgb(0x45, 0xf0, 0x7d);
/// Near-black background, `#060907`.
pub const BG: Color = Color::Rgb(0x06, 0x09, 0x07);

/// Redraw cap: poll for input up to this long, giving ~30 Hz when idle.
const FRAME: Duration = Duration::from_millis(33);

/// The outcome of handling an input event — what the event loop should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    ConfirmQuit,
    ToggleHelp,
    Focus,
    ToggleCrate,
    Filter,
    CrateNav,
    /// A track (crate selection or demo) is pending decode onto a pad.
    AssignPad,
    TriggerPad,
    Mark,
    Record,
    MasterGain,
    Timeline,
}

/// Which grid cell currently has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Crate,
    Pad(usize),
}

/// State of the keyboard-first pads UI.
pub struct App {
    pub show_help: bool,
    pub should_quit: bool,
    pub confirm_quit: bool,
    pub mixer: Mixer,
    focus: Focus,
    /// Last pad that held focus — the target for crate loads from elsewhere.
    last_pad: usize,
    crate_lib: Crate,
    crate_sel: usize,
    filter: Option<String>,
    crate_collapsed: bool,
    /// A track pending background decode onto a pad: `(pad, path)`.
    pending_pad_load: Option<(usize, PathBuf)>,
    /// Per-pad "decoding in the background" flag, for the loading indicator.
    loading: [bool; PADS],
    /// Clips recorded this session (resamples of the live mix), newest last.
    recordings: Vec<Clip>,
    /// Detected BPM per file path, so reloading skips re-analysis.
    bpm_cache: HashMap<PathBuf, f32>,
    /// Frame counter, for the DJ cat's idle bob.
    tick: u64,
    /// Track pending a delete confirmation, if the modal is open.
    confirm_delete: Option<PathBuf>,
    /// Active rename: `(target path, new-name buffer)`.
    rename: Option<(PathBuf, String)>,
    /// A track marked for move (cut); `p` pastes it into the current folder.
    move_mark: Option<PathBuf>,
    /// The scratch pad currently recording a phrase (taps append to it).
    phrase_rec: Option<usize>,
    /// Source file each pad was loaded from (for save-over).
    pad_source: [Option<PathBuf>; PADS],
    /// The arrangement grid, and whether its editor is showing.
    timeline: Timeline,
    tl_visible: bool,
    tl_lane: usize,
    tl_step: usize,
    /// First endpoint of a loop region being drawn (`v` marks, `v` fills).
    tl_region_start: Option<usize>,
    /// Clip-edit modal: the pad being edited + which mark is active
    /// (`false` = in/left, `true` = out/right).
    clip_edit: Option<usize>,
    ce_out: bool,
    /// Arrangement transport.
    playing: bool,
    play_acc: f64,          // frames accumulated toward the next step
    play_step: usize,       // next step to fire
    prev_run: [bool; PADS], // whether each lane was in a run last step
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        App {
            show_help: false,
            should_quit: false,
            confirm_quit: false,
            mixer: Mixer::new(),
            focus: Focus::Pad(0),
            last_pad: 0,
            crate_lib: Crate::from_entries(Vec::new()),
            crate_sel: 0,
            filter: None,
            crate_collapsed: false,
            pending_pad_load: None,
            loading: [false; PADS],
            recordings: Vec::new(),
            bpm_cache: HashMap::new(),
            tick: 0,
            confirm_delete: None,
            rename: None,
            move_mark: None,
            phrase_rec: None,
            pad_source: std::array::from_fn(|_| None),
            timeline: Timeline::default(),
            tl_visible: false,
            tl_lane: 0,
            tl_step: 0,
            tl_region_start: None,
            clip_edit: None,
            ce_out: false,
            playing: false,
            play_acc: 0.0,
            play_step: 0,
            prev_run: [false; PADS],
        }
    }

    /// Play / **pause** the transport. Pausing stops sound and holds the
    /// position; resuming continues from there (not the top).
    fn toggle_transport(&mut self) -> Action {
        if self.playing {
            self.playing = false;
            self.silence_pads(); // pause = go quiet, keep play_step
        } else {
            self.playing = true;
            self.prev_run = [false; PADS]; // re-fire the current step's pads
            self.play_acc = self.frames_per_step(); // fire promptly
        }
        Action::Timeline
    }

    /// `space` — master play/pause: freeze + silence the whole mix (live pads
    /// and the arrangement), or resume.
    fn toggle_global_pause(&mut self) -> Action {
        let p = self.mixer.is_paused();
        self.mixer.set_paused(!p);
        Action::Mark
    }

    /// Stop and rewind the transport to the top (Backspace).
    fn stop_transport(&mut self) -> Action {
        self.playing = false;
        self.silence_pads();
        self.play_step = 0;
        self.play_acc = 0.0;
        self.prev_run = [false; PADS];
        Action::Timeline
    }

    /// Fade every pad out (used on pause/stop so the mix goes quiet).
    fn silence_pads(&mut self) {
        for i in 0..PADS {
            self.mixer.set_pad_active(i, false, true);
        }
    }

    /// Frames per timeline step at the effective tempo (4/4: steps_per_bar/4
    /// steps per beat). 0 if tempo/grid are degenerate.
    fn frames_per_step(&self) -> f64 {
        let bpm = self.mixer.effective_bpm().unwrap_or(120.0) as f64;
        let rate = self.mixer.sample_rate() as f64;
        let steps_per_beat = self.timeline.steps_per_bar() as f64 / 4.0;
        if bpm <= 0.0 || steps_per_beat <= 0.0 {
            return 0.0;
        }
        (rate * 60.0 / bpm) / steps_per_beat
    }

    /// Advance the transport by `frames`, firing pads as their steps arrive.
    /// Driven by the audio pump so playback stays in tempo.
    pub fn advance_playback(&mut self, frames: usize) {
        if !self.playing || self.mixer.is_paused() {
            return;
        }
        let fps = self.frames_per_step();
        let total = self.timeline.total_steps();
        if fps <= 0.0 || total == 0 {
            return;
        }
        self.play_acc += frames as f64;
        while self.play_acc >= fps {
            self.play_acc -= fps;
            let step = self.play_step % total;
            self.fire_step(step);
            self.play_step = (self.play_step + 1) % total;
        }
    }

    /// Fire the pads entering/leaving a run at `step`: one-shots/scratch on
    /// entry, loops triggered on entry and faded out when the run ends.
    fn fire_step(&mut self, step: usize) {
        for lane in 0..PADS {
            let now = self.timeline.step(lane, step);
            let was = self.prev_run[lane];
            if now && !was {
                match self.mixer.pad_kind(lane) {
                    PadKind::Scratch => self.mixer.play_phrase(lane),
                    _ => self.mixer.trigger_pad(lane),
                }
            } else if !now && was && self.mixer.pad_kind(lane) == PadKind::Loop {
                self.mixer.set_pad_active(lane, false, true);
            }
            self.prev_run[lane] = now;
        }
    }

    /// Render the whole arrangement to interleaved-stereo by playing it once
    /// offline (via the mixer's recorder). Transport state is preserved.
    pub fn render_arrangement(&mut self) -> Vec<f32> {
        let fps = self.frames_per_step();
        let total = self.timeline.total_steps();
        if fps <= 0.0 || total == 0 {
            return Vec::new();
        }
        let total_frames = (fps * total as f64).round() as usize;
        let saved = (self.playing, self.play_acc, self.play_step, self.prev_run);

        self.playing = true;
        self.play_step = 0;
        self.prev_run = [false; PADS];
        self.play_acc = fps; // step 0 fires immediately
        self.mixer.arm_record();

        let block = 512usize;
        let mut scratch = vec![0.0f32; block * 2];
        let mut done = 0;
        while done < total_frames {
            let n = block.min(total_frames - done);
            self.advance_playback(n);
            scratch.resize(n * 2, 0.0);
            self.mixer.fill_mix(&mut scratch);
            done += n;
        }
        let out = self.mixer.take_recording();
        (self.playing, self.play_acc, self.play_step, self.prev_run) = saved;
        out
    }

    /// `w` (in the editor) — render the arrangement to a WAV in the current
    /// library folder, then refresh the list so it appears.
    fn render_to_library(&mut self) -> Action {
        let samples = self.render_arrangement();
        if samples.is_empty() {
            return Action::None;
        }
        let dir = self.crate_lib.cwd().to_path_buf();
        let mut n = 1;
        let path = loop {
            let p = dir.join(format!("mix-{n}.wav"));
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        let _ = termkrush_core::audio::write_wav(&path, &samples, self.mixer.sample_rate(), 2);
        self.crate_lib.refresh();
        Action::Record
    }

    /// The step the playhead last fired (for the editor's playhead marker).
    fn playhead(&self) -> Option<usize> {
        if self.playing {
            let total = self.timeline.total_steps().max(1);
            Some((self.play_step + total - 1) % total)
        } else {
            None
        }
    }

    /// Handle a key while the timeline editor is showing. Returns `None` to
    /// fall through to the normal handler (for keys the editor ignores).
    fn on_timeline_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('t') | KeyCode::Esc => {
                self.tl_visible = false;
                Some(Action::Timeline)
            }
            KeyCode::Up => {
                self.tl_lane = self.tl_lane.saturating_sub(1);
                Some(Action::Timeline)
            }
            KeyCode::Down => {
                self.tl_lane = (self.tl_lane + 1).min(PADS - 1);
                Some(Action::Timeline)
            }
            KeyCode::Left => {
                self.tl_step = self.tl_step.saturating_sub(1);
                Some(Action::Timeline)
            }
            KeyCode::Right => {
                self.tl_step = (self.tl_step + 1).min(self.timeline.total_steps() - 1);
                Some(Action::Timeline)
            }
            KeyCode::Enter => {
                self.timeline.toggle(self.tl_lane, self.tl_step);
                Some(Action::Timeline)
            }
            KeyCode::Char(' ') => Some(self.toggle_global_pause()),
            KeyCode::Char('p') => Some(self.toggle_transport()),
            KeyCode::Backspace => Some(self.stop_transport()),
            KeyCode::Char('x') => {
                self.timeline.cut_at(self.tl_step);
                self.tl_step = self
                    .tl_step
                    .min(self.timeline.total_steps().saturating_sub(1));
                Some(Action::Timeline)
            }
            KeyCode::Char('w') => Some(self.render_to_library()),
            KeyCode::Char('v') => {
                // First `v` marks the region start; second fills to the cursor.
                match self.tl_region_start {
                    None => self.tl_region_start = Some(self.tl_step),
                    Some(s) => {
                        self.timeline.fill_region(self.tl_lane, s, self.tl_step);
                        self.tl_region_start = None;
                    }
                }
                Some(Action::Timeline)
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                Some(Action::ToggleHelp)
            }
            KeyCode::Char('q') => {
                self.confirm_quit = true;
                Some(Action::ConfirmQuit)
            }
            _ => None,
        }
    }

    /// `x` — arm a delete confirmation for the highlighted track.
    fn arm_delete(&mut self) -> Action {
        if self.focus == Focus::Crate {
            if let Some(p) = self.selected_path() {
                self.confirm_delete = Some(p);
                return Action::Mark;
            }
        }
        Action::None
    }

    /// `R` — begin renaming the highlighted track.
    fn start_rename(&mut self) -> Action {
        if self.focus == Focus::Crate {
            if let Some(p) = self.selected_path() {
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                self.rename = Some((p, name));
                return Action::Filter;
            }
        }
        Action::None
    }

    /// `m` — mark the highlighted track for a move (cut).
    fn mark_move(&mut self) -> Action {
        if self.focus == Focus::Crate {
            if let Some(p) = self.selected_path() {
                self.move_mark = Some(p);
                return Action::Mark;
            }
        }
        Action::None
    }

    /// `p` — move the marked track into the current folder.
    fn paste_move(&mut self) -> Action {
        if let Some(p) = self.move_mark.take() {
            let dir = self.crate_lib.cwd().to_path_buf();
            let _ = self.crate_lib.move_into(&p, &dir);
            self.crate_sel = 0;
            Action::Mark
        } else {
            Action::None
        }
    }

    pub fn set_crate(&mut self, c: Crate) {
        self.crate_lib = c;
        self.crate_sel = 0;
    }

    pub fn focus_cell(&self) -> Focus {
        self.focus
    }

    /// The pad the current focus acts on (the focused pad, else the last one).
    fn active_pad(&self) -> usize {
        match self.focus {
            Focus::Pad(i) => i,
            _ => self.last_pad,
        }
    }

    // ---- focus order: Crate, Pad0..PADS-1 ----------------------------------

    fn focus_order() -> Vec<Focus> {
        let mut v = vec![Focus::Crate];
        v.extend((0..PADS).map(Focus::Pad));
        v
    }

    fn focus_index(&self) -> usize {
        Self::focus_order()
            .iter()
            .position(|&f| f == self.focus)
            .unwrap_or(0)
    }

    fn set_focus(&mut self, f: Focus) {
        if let Focus::Pad(i) = f {
            self.last_pad = i;
        }
        self.focus = f;
    }

    fn step_focus(&mut self, delta: isize) -> Action {
        let order = Self::focus_order();
        let n = order.len() as isize;
        let i = (self.focus_index() as isize + delta).rem_euclid(n) as usize;
        self.set_focus(order[i]);
        Action::Focus
    }

    // ---- crate browsing ----------------------------------------------------

    /// Track-list entries after the active filter, as `(name, path, is_dir)`.
    fn filtered(&self) -> Vec<(String, PathBuf, bool)> {
        let q = self.filter.as_deref().unwrap_or("");
        self.crate_lib
            .filtered(q)
            .into_iter()
            .map(|e| (e.name.clone(), e.path.clone(), e.is_dir))
            .collect()
    }

    /// The highlighted entry's `(path, is_dir)`, if any.
    fn selected(&self) -> Option<(PathBuf, bool)> {
        let f = self.filtered();
        f.get(self.crate_sel.min(f.len().saturating_sub(1)))
            .map(|(_, p, d)| (p.clone(), *d))
    }

    /// Highlighted track path (folders excluded), if any.
    pub fn selected_path(&self) -> Option<PathBuf> {
        match self.selected() {
            Some((p, false)) => Some(p),
            _ => None,
        }
    }

    fn selected_is_dir(&self) -> bool {
        matches!(self.selected(), Some((_, true)))
    }

    /// Navigate into the highlighted folder (including `..`).
    fn enter_selected(&mut self) -> Action {
        if let Some((p, true)) = self.selected() {
            self.crate_lib.enter(&p);
            self.crate_sel = 0;
            Action::CrateNav
        } else {
            Action::None
        }
    }

    fn crate_nav(&mut self, delta: isize) -> Action {
        let len = self.filtered().len();
        if len == 0 {
            self.crate_sel = 0;
        } else {
            let i = (self.crate_sel as isize + delta).rem_euclid(len as isize) as usize;
            self.crate_sel = i;
        }
        Action::CrateNav
    }

    // ---- loading a track onto a pad ---------------------------------------

    /// Queue the highlighted crate track to decode onto pad `pad`.
    fn load_selected_onto(&mut self, pad: usize) -> Action {
        match self.selected_path() {
            Some(p) => {
                self.pending_pad_load = Some((pad, p));
                Action::AssignPad
            }
            None => Action::None,
        }
    }

    fn take_pending_pad_load(&mut self) -> Option<(usize, PathBuf)> {
        self.pending_pad_load.take()
    }

    // ---- pad actions -------------------------------------------------------

    /// Fire pad `pad` (`j`): on a scratch pad, append+play a wiki while
    /// recording a phrase, else play its phrase; otherwise a normal trigger.
    fn trigger(&mut self, pad: usize) -> Action {
        if self.mixer.pad_kind(pad) == PadKind::Scratch {
            if self.phrase_rec == Some(pad) {
                self.mixer.push_phrase(pad, ScratchUnit::Wiki);
                self.mixer.scratch_wiki(pad);
            } else {
                self.mixer.play_phrase(pad);
            }
        } else {
            self.mixer.trigger_pad(pad);
        }
        Action::TriggerPad
    }

    /// `k` — whip on a scratch pad (append while recording), else assign the
    /// latest recording.
    fn secondary(&mut self) -> Action {
        if let Focus::Pad(i) = self.focus {
            if self.mixer.pad_kind(i) == PadKind::Scratch {
                if self.phrase_rec == Some(i) {
                    self.mixer.push_phrase(i, ScratchUnit::Whip);
                }
                self.mixer.scratch_whip(i);
                return Action::TriggerPad;
            }
        }
        self.assign_recording()
    }

    /// `C` — clear the focused scratch pad's phrase.
    fn clear_phrase(&mut self) -> Action {
        if let Focus::Pad(i) = self.focus {
            if self.mixer.pad_kind(i) == PadKind::Scratch {
                self.mixer.clear_phrase(i);
                return Action::Mark;
            }
        }
        Action::None
    }

    /// `P` — toggle phrase-record on the focused scratch pad (clears on arm).
    fn toggle_phrase_rec(&mut self) -> Action {
        if let Focus::Pad(i) = self.focus {
            if self.mixer.pad_kind(i) == PadKind::Scratch {
                if self.phrase_rec == Some(i) {
                    self.phrase_rec = None;
                } else {
                    self.mixer.clear_phrase(i);
                    self.phrase_rec = Some(i);
                }
                return Action::Mark;
            }
        }
        Action::None
    }

    /// `e` — open the clip-edit modal on the focused (loaded) pad.
    fn open_clip_edit(&mut self) -> Action {
        if let Focus::Pad(i) = self.focus {
            if self.mixer.pad_loaded(i) {
                self.clip_edit = Some(i);
                self.ce_out = false; // start on the in (left) mark
                return Action::Mark;
            }
        }
        Action::None
    }

    /// Handle a key while the clip-edit modal is open. `Tab` switches the
    /// active mark (in/out), `←/→` move it (shift = coarse), `space` auditions
    /// the selection, `x` snips both sides, `e`/esc close.
    fn on_clip_key(&mut self, key: KeyEvent) -> Option<Action> {
        let i = self.clip_edit?;
        let rate = self.mixer.sample_rate() as usize;
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let step = (if shift { rate / 10 } else { rate / 100 }).max(1) as i64; // 100ms / 10ms
        let (inp, out) = self.mixer.pad_trim(i);
        let nudge = |v: usize, d: i64| (v as i64 + d).max(0) as usize;
        match key.code {
            KeyCode::Char('e') | KeyCode::Esc => {
                self.clip_edit = None;
                Some(Action::Mark)
            }
            KeyCode::Tab | KeyCode::Up | KeyCode::Down => {
                self.ce_out = !self.ce_out; // switch active mark
                Some(Action::Mark)
            }
            KeyCode::Left | KeyCode::Right => {
                let d = if key.code == KeyCode::Right {
                    step
                } else {
                    -step
                };
                if self.ce_out {
                    self.mixer.set_pad_trim_out(i, nudge(out, d));
                } else {
                    self.mixer.set_pad_trim_in(i, nudge(inp, d));
                }
                Some(Action::Mark)
            }
            KeyCode::Char(' ') => {
                self.mixer.audition_pad(i); // preview the selection
                Some(Action::Mark)
            }
            KeyCode::Char('x') => {
                self.mixer.snip_pad(i); // destructively keep only [in, out]
                self.ce_out = false;
                Some(Action::Mark)
            }
            KeyCode::Char('q') => {
                self.confirm_quit = true;
                Some(Action::ConfirmQuit)
            }
            _ => None,
        }
    }

    /// `u` — unload the focused pad (clear its clip + state).
    fn unload(&mut self) -> Action {
        if let Focus::Pad(i) = self.focus {
            self.mixer.unload_pad(i);
            self.pad_source[i] = None;
            Action::Mark
        } else {
            Action::None
        }
    }

    /// `;` — cycle the focused pad's kind (one-shot / loop / scratch).
    fn cycle_kind(&mut self) -> Action {
        if let Focus::Pad(i) = self.focus {
            self.mixer.cycle_pad_kind(i);
            Action::Mark
        } else {
            Action::None
        }
    }

    /// `f` — toggle the focused pad's activation (soft fade in/out).
    fn toggle_active(&mut self) -> Action {
        if let Focus::Pad(i) = self.focus {
            let on = self.mixer.pad_active(i);
            self.mixer.set_pad_active(i, !on, true);
            Action::Mark
        } else {
            Action::None
        }
    }

    /// `-` / `=` — nudge the focused pad's volume.
    fn pad_volume(&mut self, up: bool) -> Action {
        if let Focus::Pad(i) = self.focus {
            self.mixer.nudge_pad_gain(i, if up { 0.05 } else { -0.05 });
            Action::Mark
        } else {
            Action::None
        }
    }

    /// Up/Down volume: a focused pad → its volume; anything else (the master
    /// timeline) → master volume.
    fn volume(&mut self, up: bool) -> Action {
        match self.focus {
            Focus::Pad(i) => {
                self.mixer.nudge_pad_gain(i, if up { 0.05 } else { -0.05 });
                Action::Mark
            }
            _ => {
                self.mixer
                    .nudge_master(if up { GAIN_NUDGE } else { -GAIN_NUDGE });
                Action::MasterGain
            }
        }
    }

    fn trim_in(&mut self, forward: bool, fine: bool) -> Action {
        if let Focus::Pad(i) = self.focus {
            let step = if fine { TRIM_FINE } else { TRIM_COARSE };
            self.mixer
                .nudge_pad_in(i, if forward { step } else { -step });
            Action::Mark
        } else {
            Action::None
        }
    }

    fn trim_out(&mut self, forward: bool, fine: bool) -> Action {
        if let Focus::Pad(i) = self.focus {
            let step = if fine { TRIM_FINE } else { TRIM_COARSE };
            self.mixer
                .nudge_pad_out(i, if forward { step } else { -step });
            Action::Mark
        } else {
            Action::None
        }
    }

    fn nudge_bpm(&mut self, up: bool, fine: bool) -> Action {
        if let Focus::Pad(i) = self.focus {
            let step = if fine { 0.1 } else { 1.0 } * if up { 1.0 } else { -1.0 };
            self.mixer.nudge_pad_bpm(i, step);
            Action::Mark
        } else {
            Action::None
        }
    }

    /// Assign the most-recent recording to the focused pad.
    fn assign_recording(&mut self) -> Action {
        if let (Focus::Pad(i), Some(clip)) = (self.focus, self.recordings.last()) {
            self.mixer.assign_pad(i, clip.samples.clone());
            self.mixer.set_pad_bpm(i, clip.bpm);
            Action::Mark
        } else {
            Action::None
        }
    }

    /// `r` — toggle the live-mix recorder. On disarm the capture becomes a
    /// clip: onto the focused pad if one is focused, else the stash.
    fn toggle_record(&mut self) -> Action {
        if self.mixer.is_recording() {
            let samples = self.mixer.take_recording();
            if samples.len() >= 2 {
                if let Focus::Pad(i) = self.focus {
                    self.mixer.assign_pad(i, samples);
                } else {
                    let name = format!("Mix {}", self.recordings.len() + 1);
                    self.recordings.push(Clip::new(samples, None, name));
                }
            }
        } else {
            self.mixer.arm_record();
        }
        Action::Record
    }

    fn is_loading(&self, pad: usize) -> bool {
        self.loading.get(pad).copied().unwrap_or(false)
    }

    // ---- input -------------------------------------------------------------

    pub fn on_event(&mut self, ev: &Event) -> Action {
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => self.on_key(*k),
            _ => Action::None,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Action {
        // Quit-confirm modal swallows everything until answered.
        if self.confirm_quit {
            return match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.should_quit = true;
                    Action::Quit
                }
                _ => {
                    self.confirm_quit = false;
                    Action::None
                }
            };
        }

        // Ctrl-C always force-quits.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return Action::Quit;
        }

        // Delete-confirm modal.
        if let Some(path) = self.confirm_delete.clone() {
            return match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let _ = self.crate_lib.delete(&path);
                    self.confirm_delete = None;
                    self.crate_sel = 0;
                    Action::Mark
                }
                _ => {
                    self.confirm_delete = None;
                    Action::None
                }
            };
        }

        // Rename-entry mode captures typing for the new file name.
        if self.rename.is_some() {
            return match key.code {
                KeyCode::Char(c) => {
                    self.rename.as_mut().unwrap().1.push(c);
                    Action::Filter
                }
                KeyCode::Backspace => {
                    self.rename.as_mut().unwrap().1.pop();
                    Action::Filter
                }
                KeyCode::Esc => {
                    self.rename = None;
                    Action::Filter
                }
                KeyCode::Enter => {
                    let (p, b) = self.rename.take().unwrap();
                    if !b.is_empty() {
                        let _ = self.crate_lib.rename(&p, &b);
                        self.crate_sel = 0;
                    }
                    Action::Mark
                }
                _ => Action::None,
            };
        }

        // Filter-entry mode captures typing for the crate search.
        if let Some(q) = self.filter.as_mut() {
            return match key.code {
                KeyCode::Char(c) => {
                    q.push(c);
                    self.crate_sel = 0;
                    Action::Filter
                }
                KeyCode::Backspace => {
                    q.pop();
                    Action::Filter
                }
                KeyCode::Esc => {
                    self.filter = None;
                    Action::Filter
                }
                KeyCode::Enter => {
                    self.filter = None;
                    if self.selected_is_dir() {
                        self.enter_selected()
                    } else {
                        self.load_selected_onto(self.active_pad())
                    }
                }
                KeyCode::Up => self.crate_nav(-1),
                KeyCode::Down => self.crate_nav(1),
                _ => Action::None,
            };
        }

        // Clip-edit modal captures keys while open.
        if self.clip_edit.is_some() {
            if let Some(a) = self.on_clip_key(key) {
                return a;
            }
        }

        // Timeline editor captures navigation + toggling while it's open.
        if self.tl_visible {
            if let Some(a) = self.on_timeline_key(key) {
                return a;
            }
        }

        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.confirm_quit = true;
                Action::ConfirmQuit
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
                Action::ToggleHelp
            }
            // Focus moves with Tab / Shift-Tab and Left / Right.
            KeyCode::Tab | KeyCode::Right => self.step_focus(1),
            KeyCode::BackTab | KeyCode::Left => self.step_focus(-1),

            // Up/Down = volume — except on the library, where they browse.
            KeyCode::Up if self.focus == Focus::Crate => self.crate_nav(-1),
            KeyCode::Down if self.focus == Focus::Crate => self.crate_nav(1),
            KeyCode::Up => self.volume(true),
            KeyCode::Down => self.volume(false),

            // Crate.
            KeyCode::Char('/') => {
                self.filter = Some(String::new());
                self.crate_sel = 0;
                Action::Filter
            }
            KeyCode::Char('z') => {
                self.crate_collapsed = !self.crate_collapsed;
                Action::ToggleCrate
            }
            KeyCode::Char('t') => {
                self.tl_visible = true;
                Action::Timeline
            }
            KeyCode::Char(' ') => self.toggle_global_pause(),
            // Library file ops (on the highlighted track).
            KeyCode::Char('x') => self.arm_delete(),
            KeyCode::Char('R') => self.start_rename(),
            KeyCode::Char('m') => self.mark_move(),
            KeyCode::Char('p') => self.paste_move(),
            KeyCode::Char(';') => self.cycle_kind(),
            KeyCode::Char('u') => self.unload(),
            KeyCode::Char('e') => self.open_clip_edit(),
            KeyCode::Char('f') => self.toggle_active(),
            KeyCode::Char('P') => self.toggle_phrase_rec(),
            KeyCode::Char('C') => self.clear_phrase(),
            KeyCode::Char('S') => self.save_pad_new(),
            KeyCode::Char('O') => self.save_pad_over(),
            KeyCode::Char('E') => self.export_pad_mp3(),
            KeyCode::Char('-') => self.pad_volume(false),
            KeyCode::Char('=') => self.pad_volume(true),
            KeyCode::Enter if self.focus == Focus::Crate && self.selected_is_dir() => {
                self.enter_selected()
            }
            KeyCode::Enter => self.load_selected_onto(self.active_pad()),
            KeyCode::Char('\\') => {
                self.pending_pad_load = Some((self.active_pad(), demo_track_path()));
                Action::AssignPad
            }

            // Pad action cluster (on the focused pad).
            KeyCode::Char('j') => self.trigger(self.active_pad()),
            KeyCode::Char('l') => self.load_selected_onto(self.active_pad()),
            KeyCode::Char('k') => self.secondary(),
            KeyCode::Char('a') => self.trim_in(false, shift),
            KeyCode::Char('d') => self.trim_in(true, shift),
            KeyCode::Char('w') => self.trim_out(true, shift),
            KeyCode::Char('s') => self.trim_out(false, shift),
            KeyCode::Char(',') => self.nudge_bpm(false, shift),
            KeyCode::Char('.') => self.nudge_bpm(true, shift),

            // Globals.
            KeyCode::Char('r') => self.toggle_record(),
            KeyCode::Char('}') => {
                self.mixer.nudge_global_speed(0.02);
                Action::MasterGain
            }
            KeyCode::Char('{') => {
                self.mixer.nudge_global_speed(-0.02);
                Action::MasterGain
            }
            KeyCode::Char('[') => {
                self.mixer.nudge_master(-GAIN_NUDGE);
                Action::MasterGain
            }
            KeyCode::Char(']') => {
                self.mixer.nudge_master(GAIN_NUDGE);
                Action::MasterGain
            }
            KeyCode::Char(c @ '1'..='8') => {
                let pad = c.to_digit(10).unwrap() as usize - 1;
                self.trigger(pad)
            }
            _ => Action::None,
        }
    }

    /// Apply a background-decoded track onto its pad.
    fn place_decoded(&mut self, d: Decoded) {
        let LoadTarget::Pad(i) = d.target;
        self.mixer.assign_pad(i, d.track.samples);
        if let Some(b) = d.bpm {
            self.mixer.set_pad_bpm(i, Some(b));
            self.bpm_cache.insert(d.path.clone(), b);
            // Auto-BPM: the first track with a tempo silently sets the master;
            // every loop varispeeds to it. Later tracks just adopt it — no
            // prompt, ever.
            if self.mixer.master_bpm().is_none() {
                self.mixer.set_master_bpm(Some(b));
            }
        }
        if i < PADS {
            self.loading[i] = false;
            self.pad_source[i] = Some(d.path);
        }
    }

    /// `S` — save the focused pad's trimmed clip as a NEW WAV in the library.
    fn save_pad_new(&mut self) -> Action {
        let Focus::Pad(i) = self.focus else {
            return Action::None;
        };
        let region = self.mixer.pad_clip_region(i);
        if region.is_empty() {
            return Action::None;
        }
        let stem = self.pad_source[i]
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("pad")
            .to_string();
        let dir = self.crate_lib.cwd().to_path_buf();
        let mut n = 1;
        let path = loop {
            let p = dir.join(format!("{stem}-edit{n}.wav"));
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        let _ = termkrush_core::audio::write_wav(&path, &region, self.mixer.sample_rate(), 2);
        self.crate_lib.refresh();
        Action::Record
    }

    /// `E` — export the focused pad's trimmed clip to an MP3 in the library.
    fn export_pad_mp3(&mut self) -> Action {
        let Focus::Pad(i) = self.focus else {
            return Action::None;
        };
        let region = self.mixer.pad_clip_region(i);
        if region.is_empty() {
            return Action::None;
        }
        let stem = self.pad_source[i]
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("pad")
            .to_string();
        let dir = self.crate_lib.cwd().to_path_buf();
        let mut n = 1;
        let path = loop {
            let p = dir.join(format!("{stem}-{n}.mp3"));
            if !p.exists() {
                break p;
            }
            n += 1;
        };
        let _ = termkrush_core::audio::export_mp3(&path, &region, self.mixer.sample_rate(), 2);
        self.crate_lib.refresh();
        Action::Record
    }

    /// `O` — overwrite the focused pad's source file with its trimmed clip.
    fn save_pad_over(&mut self) -> Action {
        let Focus::Pad(i) = self.focus else {
            return Action::None;
        };
        let Some(src) = self.pad_source[i].clone() else {
            return Action::None;
        };
        let region = self.mixer.pad_clip_region(i);
        if region.is_empty() {
            return Action::None;
        }
        let _ = termkrush_core::audio::write_wav(&src, &region, self.mixer.sample_rate(), 2);
        self.crate_lib.refresh();
        Action::Record
    }
}

// ---- rendering -------------------------------------------------------------

/// Shorten `s` to fit `width`, adding an ellipsis when truncated.
fn ellipsize(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else if width <= 1 {
        "…".to_string()
    } else {
        let mut out: String = s.chars().take(width - 1).collect();
        out.push('…');
        out
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    f.render_widget(Block::default().style(Style::default().bg(BG)), f.area());

    let rows = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(f.area());
    draw_header(f, rows[0], app);

    // The timeline editor takes over the body while it's open.
    if app.tl_visible {
        draw_timeline(f, rows[1], app);
        return_help(f, app);
        return;
    }
    // Clip-edit modal overlays everything else.
    if let Some(pad) = app.clip_edit {
        draw_clip_edit(f, rows[1], app, pad);
        return_help(f, app);
        return;
    }

    // Master timeline is a permanent strip across the top of the body; the
    // library + pads sit below it.
    let stack =
        Layout::vertical([Constraint::Length(PADS as u16 + 2), Constraint::Min(0)]).split(rows[1]);
    draw_timeline_strip(f, stack[0], app);
    let lower = stack[1];

    // Lower body: crate on the left (unless collapsed), pads on the right.
    let body = if app.crate_collapsed {
        let cols = Layout::horizontal([Constraint::Min(0)]).split(lower);
        draw_pads(f, cols[0], app);
        return_help(f, app);
        return;
    } else {
        Layout::horizontal([Constraint::Length(34), Constraint::Min(0)]).split(lower)
    };
    draw_crate(f, body[0], app);
    draw_pads(f, body[1], app);
    return_help(f, app);
}

/// The clip-edit modal: the focused pad's clip as a bar, with the trimmed
/// region filled, the in/out handles, and a movable cursor. Arrows move the
/// cursor; i/o set in/out; x truncates at the cursor.
fn draw_clip_edit(f: &mut Frame, area: Rect, app: &App, pad: usize) {
    let len = app.mixer.pad_clip_frames(pad).max(1);
    let (inp, out) = app.mixer.pad_trim(pad);
    let rate = app.mixer.sample_rate().max(1) as f64;
    let secs = |fr: usize| fr as f64 / rate;
    let active = if app.ce_out { "out ▸" } else { "◂ in" };
    let title = format!(
        "Edit Pad {} [{active}] — tab switch · ←/→ move · space audition · x snip · e close",
        pad + 1
    );
    let block = cell_block(&title, true);
    let inner = block.inner(area);
    let w = (inner.width as usize).saturating_sub(2).max(8);
    let col = |fr: usize| (fr.min(len) * w / len).min(w.saturating_sub(1));
    let mut bar: Vec<char> = (0..w)
        .map(|c| {
            let fr = c * len / w;
            if fr >= inp && fr < out {
                '█'
            } else {
                '░'
            }
        })
        .collect();
    // In/out handles; the active one is doubled so it stands out.
    bar[col(inp)] = if app.ce_out { '◂' } else { '◀' };
    bar[col(out.saturating_sub(1))] = if app.ce_out { '▶' } else { '▸' };
    let lines = vec![
        Line::from(""),
        Line::from(format!("  [{}]", bar.iter().collect::<String>())),
        Line::from(""),
        Line::from(format!(
            "  in {:.2}s   out {:.2}s   selection {:.2}s   (clip {:.2}s)",
            secs(inp),
            secs(out),
            secs(out.saturating_sub(inp)),
            secs(len)
        )),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(GREEN)),
        area,
    );
}

/// The tracker step-grid editor: one row per pad, columns are steps. The
/// cursor cell is boxed; bar boundaries are spaced for legibility.
fn draw_timeline(f: &mut Frame, area: Rect, app: &App) {
    let tl = &app.timeline;
    let spb = tl.steps_per_bar();
    let region = if app.tl_region_start.is_some() {
        "  [v: set region end]"
    } else {
        ""
    };
    let transport = if app.playing { "▶" } else { "■" };
    let head = app.playhead();
    let title = format!(
        "Timeline {transport} {}×{}  (enter hit · v region · space play · t close){region}",
        tl.bars(),
        spb
    );
    let block = cell_block(&title, true);
    let mut lines: Vec<Line> = Vec::with_capacity(PADS);
    for lane in 0..PADS {
        let mut s = format!(
            "{}P{} ",
            if lane == app.tl_lane { "▸" } else { " " },
            lane + 1
        );
        for step in 0..tl.total_steps() {
            if step > 0 && step % spb == 0 {
                s.push('|'); // bar boundary
            }
            let hit = tl.step(lane, step);
            let cursor = lane == app.tl_lane && step == app.tl_step;
            let on_head = head == Some(step);
            s.push(match (cursor, hit, on_head) {
                (true, true, _) => '▣',
                (true, false, _) => '▢',
                (false, true, true) => '▶',
                (false, true, false) => '█',
                (false, false, true) => ':',
                (false, false, false) => '·',
            });
        }
        lines.push(Line::from(s));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(GREEN)),
        area,
    );
}

/// The persistent master-timeline strip at the top of the body: one lane per
/// pad, hits as `█`, the playhead as `▶`/`:`. Read-only overview — `t` opens
/// the full editor.
fn draw_timeline_strip(f: &mut Frame, area: Rect, app: &App) {
    let tl = &app.timeline;
    let spb = tl.steps_per_bar();
    let transport = if app.playing { "▶" } else { "■" };
    let title = format!("TIMELINE {transport} {}×{}  ·  t edit", tl.bars(), spb);
    let block = cell_block(&title, false);
    let head = app.playhead();
    let mut lines: Vec<Line> = Vec::with_capacity(PADS);
    for lane in 0..PADS {
        let mut s = format!(" P{} ", lane + 1);
        for step in 0..tl.total_steps() {
            if step > 0 && step % spb == 0 {
                s.push('|'); // bar boundary
            }
            s.push(match (tl.step(lane, step), head == Some(step)) {
                (true, true) => '▶',
                (true, false) => '█',
                (false, true) => ':',
                (false, false) => '·',
            });
        }
        lines.push(Line::from(s));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(GREEN)),
        area,
    );
}

fn return_help(f: &mut Frame, app: &App) {
    if app.confirm_quit {
        draw_quit_modal(f, f.area());
    } else if let Some(path) = &app.confirm_delete {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("track");
        draw_confirm_modal(f, f.area(), &format!("Delete {name}?"));
    } else if app.show_help {
        draw_help(f, f.area());
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let tempo = match app.mixer.effective_bpm() {
        Some(b) => format!("TermKrush   ♩ {b:.0} BPM"),
        None => "TermKrush".to_string(),
    };
    let lines = vec![
        Line::from(Span::styled(
            tempo,
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ))
        .centered(),
        Line::from(Span::styled(
            "tab focus  j play  l load  a/d/w/s trim  r record  ? help",
            Style::default().fg(GREEN),
        ))
        .centered(),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_crate(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus_cell() == Focus::Crate;
    let entries = app.filtered();
    let title = if let Some((_, buf)) = &app.rename {
        format!("Library  rename: {buf}")
    } else if app.move_mark.is_some() {
        format!("Library  ({} items) [move: p to paste]", entries.len())
    } else {
        match &app.filter {
            Some(q) => format!("Library  /{q}"),
            None => format!("Library  ({} items)", entries.len()),
        }
    };
    let block = cell_block(&title, focused);
    let inner = block.inner(area);
    let items: Vec<ListItem> = entries
        .iter()
        .map(|(name, _, is_dir)| {
            let label = if *is_dir && name != ".." {
                format!("{name}/")
            } else {
                name.clone()
            };
            ListItem::new(ellipsize(&label, inner.width as usize))
        })
        .collect();
    let mut state = ListState::default();
    if !entries.is_empty() {
        state.select(Some(app.crate_sel.min(entries.len() - 1)));
    }
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().fg(AMBER).add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, area, &mut state);
    if entries.is_empty() {
        let hint = Paragraph::new("No tracks. Set crate_root in config.toml.")
            .style(Style::default().fg(GREEN))
            .wrap(Wrap { trim: true });
        let r = Rect::new(inner.x, inner.y, inner.width, inner.height.min(3));
        f.render_widget(hint, r);
    }
}

fn draw_pads(f: &mut Frame, area: Rect, app: &App) {
    // Four rows of two cells: pads 1..8.
    let rows = Layout::vertical([Constraint::Ratio(1, 4); 4]).split(area);
    for (row, chunk) in rows.iter().enumerate() {
        let cols = Layout::horizontal([Constraint::Ratio(1, 2); 2]).split(*chunk);
        for (col, cell) in cols.iter().enumerate() {
            let idx = row * 2 + col;
            if idx < PADS {
                draw_pad_cell(f, *cell, app, idx);
            }
        }
    }
}

fn cell_block(title: &str, focused: bool) -> Block<'static> {
    let marker = if focused { "▸ " } else { "" };
    let color = if focused { AMBER } else { GREEN };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!("{marker}{title}"),
            Style::default().fg(color),
        ))
}

fn draw_cell(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line<'static>>, focused: bool) {
    let block = cell_block(title, focused);
    f.render_widget(Paragraph::new(lines).block(block), area);
}

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

fn draw_pad_cell(f: &mut Frame, area: Rect, app: &App, pad: usize) {
    let focused = app.focus_cell() == Focus::Pad(pad);
    let loaded = app.mixer.pad_loaded(pad);
    let glyph = if loaded { '●' } else { '·' };
    let kind = match app.mixer.pad_kind(pad) {
        PadKind::OneShot => "1shot",
        PadKind::Loop => "loop",
        PadKind::Scratch => "scratch",
    };
    let off = if app.mixer.pad_active(pad) {
        ""
    } else {
        " off"
    };
    let line1 = if app.is_loading(pad) {
        "  ⏳ loading…".to_string()
    } else {
        format!(
            "  {glyph} {kind} {:.0}%{off}",
            app.mixer.pad_gain(pad) * 100.0
        )
    };
    let line2 = if loaded && app.mixer.pad_kind(pad) == PadKind::Scratch {
        let rec = if app.phrase_rec == Some(pad) {
            " REC"
        } else {
            ""
        };
        let ph = app.mixer.pad_phrase_glyphs(pad);
        let shown: String = {
            let mut c: Vec<char> = ph.chars().collect();
            if c.len() > 8 {
                c = c.split_off(c.len() - 8);
            }
            c.into_iter().collect()
        };
        format!("  {shown}{rec}")
    } else if focused && loaded {
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
    draw_cell(
        f,
        area,
        &format!("Pad {}", pad + 1),
        vec![Line::from(line1), Line::from(line2)],
        focused,
    );
}

/// The DJ tile's two-line 8-bit cat — bobs while voices play, else rests.
/// A small centered "Quit?" confirmation modal. `y` quits, anything else cancels.
fn draw_quit_modal(f: &mut Frame, area: Rect) {
    draw_confirm_modal(f, area, "Quit TermKrush?");
}

/// A small centered yes/no modal. `y` confirms, anything else cancels.
fn draw_confirm_modal(f: &mut Frame, area: Rect, prompt: &str) {
    let popup = centered((prompt.len() as u16 + 10).max(24), 5, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(AMBER))
        .title(" Confirm ");
    let body = Paragraph::new(vec![
        Line::from(""),
        Line::from(format!("{prompt}  y / n")).centered(),
    ])
    .block(block)
    .style(Style::default().fg(GREEN));
    f.render_widget(body, popup);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let popup = centered(56, 17, area);
    f.render_widget(Clear, popup);
    let text = "\
  focus   tab · ←/→   (library · pads)   vol ↑/↓
  library / filter   ↑↓ browse   enter open/load→pad   z hide
  files   x delete   R rename   m mark / p move-here
  pad     j play/wiki   k whip   f on/off   l load   u unload   e edit   ; kind   S save   O over   E mp3
  edit    tab switch in/out   ←/→ move   space audition   x snip   e close
  trim    a/d in   w/s out   (shift = fine)
  tempo   , / .  pad bpm
  mix     1-7 trigger   space pause   r record   - / = vol   [ ] master   { } tempo
  arrange t timeline (enter hit · v region · x cut · p play · backspace stop · w render)\n  quit    esc (y/n)   C-c force   ? help";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(AMBER))
        .title(" Help ");
    f.render_widget(
        Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(GREEN)),
        popup,
    );
}

fn centered(w: u16, h: u16, area: Rect) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

// ---- background decode -----------------------------------------------------

/// Where a freshly-decoded track is headed (pads only).
#[derive(Debug, Clone, Copy)]
enum LoadTarget {
    Pad(usize),
}

/// A track that finished decoding off-thread, on its way to a pad.
struct Decoded {
    target: LoadTarget,
    track: DecodedAudio,
    path: PathBuf,
    bpm: Option<f32>,
}

fn spawn_decode(
    target: LoadTarget,
    path: PathBuf,
    target_rate: u32,
    detect: bool,
    cached_bpm: Option<f32>,
    tx: Sender<Decoded>,
) {
    std::thread::spawn(
        move || match termkrush_core::audio::decode_file(&path, target_rate) {
            Ok(track) => {
                let bpm = cached_bpm.or_else(|| {
                    if detect {
                        termkrush_core::audio::detect_bpm(
                            &track.samples,
                            track.channels,
                            track.sample_rate,
                        )
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

/// Carry out an `AssignPad` action: decode the pending track onto its pad.
/// Returns whether a decode was kicked off. Lifted out of the loop so it is
/// testable without a tty.
fn apply_pad_assign(
    app: &mut App,
    action: Action,
    target_rate: u32,
    load_tx: &Sender<Decoded>,
) -> bool {
    if action != Action::AssignPad {
        return false;
    }
    let Some((pad, path)) = app.take_pending_pad_load() else {
        return false;
    };
    if pad < PADS {
        app.loading[pad] = true;
    }
    let cached = app.bpm_cache.get(&path).copied();
    spawn_decode(
        LoadTarget::Pad(pad),
        path,
        target_rate,
        cached.is_none(),
        cached,
        load_tx.clone(),
    );
    true
}

/// The bundled demo track path (`$TERMKRUSH_DEMO_TRACK`, else a default).
fn demo_track_path() -> PathBuf {
    std::env::var_os("TERMKRUSH_DEMO_TRACK")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/demo.mp3"))
}

// ---- terminal lifecycle ----------------------------------------------------

type Term = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

/// Restores the terminal on drop — including during a panic unwind.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn install_panic_restore() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        prev(info);
    }));
}

pub fn run() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;
    install_panic_restore();
    event_loop(&mut terminal)
}

fn event_loop(terminal: &mut Term) -> io::Result<()> {
    let mut app = App::new();

    let cfg = Config::load();
    let crate_lib = Crate::scan(&cfg.crate_root);
    tracing::info!(root = %cfg.crate_root.display(), tracks = crate_lib.len(), "crate scanned");
    app.set_crate(crate_lib);

    let (audio_out, mut producer) = match AudioOutput::start(1 << 13) {
        Ok((out, prod)) => (Some(out), Some(prod)),
        Err(e) => {
            tracing::warn!(error = %e, "audio output unavailable; running without sound");
            (None, None)
        }
    };
    let out_channels = audio_out.as_ref().map(|o| o.channels).unwrap_or(2);
    let target_rate = audio_out.as_ref().map(|o| o.sample_rate).unwrap_or(44_100);
    app.mixer.set_sample_rate(target_rate);
    let mut scratch: Vec<f32> = Vec::new();

    let (load_tx, load_rx) = std::sync::mpsc::channel::<Decoded>();

    tracing::info!("tui event loop started");
    while !app.should_quit {
        terminal.draw(|f| draw(f, &app))?;
        if event::poll(FRAME)? {
            let ev = event::read()?;
            let action = app.on_event(&ev);
            apply_pad_assign(&mut app, action, target_rate, &load_tx);
        }
        while let Ok(decoded) = load_rx.try_recv() {
            app.place_decoded(decoded);
        }
        if let Some(p) = producer.as_mut() {
            // Advance the arrangement over the same frames we're about to
            // render, so transport stays in tempo with the audio.
            let frames = p.slots() / (out_channels.max(1) as usize);
            app.advance_playback(frames);
            pump(&mut app.mixer, p, out_channels, &mut scratch);
        }
        app.tick = app.tick.wrapping_add(1);
    }
    tracing::info!("tui event loop exited");
    drop(audio_out);
    Ok(())
}

/// Render the mixed output and push it into the ring, mapping to the device's
/// channel count. Writes only what the ring has room for, so it never blocks.
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
    mixer.fill_mix(scratch);
    for f in 0..frames {
        let (l, r) = (scratch[f * 2], scratch[f * 2 + 1]);
        for ch in 0..channels {
            let s = match ch {
                0 => l,
                1 => r,
                _ => 0.0,
            };
            let _ = producer.push(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use termkrush_core::library::CrateEntry;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn render(app: &App, w: u16, h: u16) -> Buffer {
        let mut t = Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        t.backend().buffer().clone()
    }

    fn buffer_text(b: &Buffer) -> String {
        let mut s = String::new();
        for y in 0..b.area.height {
            for x in 0..b.area.width {
                s.push_str(b[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    fn demo_crate() -> Crate {
        Crate::from_entries(vec![
            CrateEntry {
                name: "Alpha.mp3".into(),
                path: "/m/a.mp3".into(),
                is_dir: false,
            },
            CrateEntry {
                name: "Beta.mp3".into(),
                path: "/m/b.mp3".into(),
                is_dir: false,
            },
        ])
    }

    #[test]
    fn x_then_y_deletes_the_highlighted_track() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-del-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("gone.wav"), b"x").unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        app.set_focus(Focus::Crate);
        assert_eq!(app.on_key(key('x')), Action::Mark); // arm
        assert!(app.confirm_delete.is_some());
        app.on_key(key('y')); // confirm
        assert!(!tmp.join("gone.wav").exists(), "file deleted");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn mark_and_paste_moves_a_track_into_a_folder() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-mv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("box")).unwrap();
        fs::write(tmp.join("t.wav"), b"x").unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        app.set_focus(Focus::Crate);
        // Entries: ["box", "t.wav"] — move to the track, mark it.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected_path(), Some(tmp.join("t.wav")));
        assert_eq!(app.on_key(key('m')), Action::Mark);
        // Back to the folder, enter it, paste.
        app.crate_sel = 0;
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // into box/
        assert_eq!(app.on_key(key('p')), Action::Mark);
        assert!(tmp.join("box/t.wav").exists() && !tmp.join("t.wav").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn folder_entries_are_not_loadable_as_tracks() {
        let mut app = App::new();
        app.set_crate(Crate::from_entries(vec![CrateEntry {
            name: "sub".into(),
            path: "/m/sub".into(),
            is_dir: true,
        }]));
        app.set_focus(Focus::Pad(0));
        // A folder is highlighted; `l` must not queue it as a track load.
        assert_eq!(app.selected_path(), None, "folders aren't track paths");
        assert_eq!(app.on_key(key('l')), Action::None);
    }

    #[test]
    fn tab_cycles_focus_through_crate_and_pads() {
        let mut app = App::new();
        app.set_focus(Focus::Crate);
        assert_eq!(app.focus_cell(), Focus::Crate);
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Action::Focus
        );
        assert_eq!(app.focus_cell(), Focus::Pad(0));
        // Step to the last pad and wrap back to Crate.
        for _ in 0..PADS - 1 {
            app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        }
        assert_eq!(app.focus_cell(), Focus::Pad(PADS - 1));
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus_cell(), Focus::Crate);
    }

    #[test]
    fn crate_browse_and_load_targets_the_focused_pad() {
        let mut app = App::new();
        app.set_crate(demo_crate());
        app.set_focus(Focus::Pad(3));
        assert_eq!(app.selected_path(), Some("/m/a.mp3".into()));
        // `l` queues the highlighted track onto the focused pad.
        assert_eq!(app.on_key(key('l')), Action::AssignPad);
        assert_eq!(app.take_pending_pad_load(), Some((3, "/m/a.mp3".into())));
    }

    #[test]
    fn filter_narrows_the_crate() {
        let mut app = App::new();
        app.set_crate(demo_crate());
        app.set_focus(Focus::Crate);
        app.on_key(key('/'));
        app.on_key(key('b')); // matches "Beta"
        assert_eq!(app.selected_path(), Some("/m/b.mp3".into()));
    }

    #[test]
    fn brace_keys_nudge_global_tempo() {
        let mut app = App::new();
        app.mixer.set_master_bpm(Some(120.0));
        assert_eq!(app.mixer.global_speed(), 1.0);
        assert_eq!(app.on_key(key('}')), Action::MasterGain);
        assert!(app.mixer.global_speed() > 1.0);
        assert!(app.mixer.effective_bpm().unwrap() > 120.0);
        assert_eq!(app.on_key(key('{')), Action::MasterGain);
    }

    #[test]
    fn f_toggles_pad_activation() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.set_focus(Focus::Pad(0));
        assert!(app.mixer.pad_active(0));
        assert_eq!(app.on_key(key('f')), Action::Mark);
        assert!(!app.mixer.pad_active(0));
    }

    #[test]
    fn clip_edit_marks_audition_and_snips_both_sides() {
        let mut app = App::new();
        app.mixer.set_sample_rate(1000); // 10ms = 10 frames
        app.mixer.assign_pad(0, vec![0.5; 4000]); // 2000 frames
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.on_key(key('e')), Action::Mark); // open (in mark active)
        assert!(!app.ce_out);
        // Move the IN mark right 5×10 = 50 frames.
        for _ in 0..5 {
            app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        }
        assert_eq!(app.mixer.pad_trim(0).0, 50, "in mark moved");
        // Tab to the OUT mark, pull it left 5×10 = 50 → out 1950.
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(app.ce_out);
        for _ in 0..5 {
            app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        }
        assert_eq!(app.mixer.pad_trim(0).1, 1950, "out mark moved");
        // Audition plays the selection.
        app.on_key(key(' '));
        assert_eq!(app.mixer.active_voices(), 1, "audition the selection");
        // Snip keeps only [50, 1950) = 1900 frames (both sides dropped).
        app.on_key(key('x'));
        assert_eq!(
            app.mixer.pad_clip_frames(0),
            1900,
            "snipped to the selection"
        );
        assert_eq!(app.on_key(key('e')), Action::Mark);
        assert!(app.clip_edit.is_none());
    }

    #[test]
    fn timeline_x_cuts_the_arrangement() {
        let mut app = App::new();
        app.on_key(key('t')); // open timeline
        app.timeline.set_step(0, 60, true);
        for _ in 0..20 {
            app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        }
        app.on_key(key('x')); // cut at step 20 → bar 2 → 32 steps
        assert_eq!(app.timeline.total_steps(), 32);
        assert!(!app.timeline.step(0, 60));
    }

    #[test]
    fn u_unloads_the_focused_pad() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.mixer.cycle_pad_kind(0); // → Loop
        app.set_focus(Focus::Pad(0));
        assert!(app.mixer.pad_loaded(0));
        assert_eq!(app.on_key(key('u')), Action::Mark);
        assert!(!app.mixer.pad_loaded(0), "pad cleared");
        assert_eq!(app.mixer.pad_kind(0), PadKind::OneShot, "kind reset");
        assert!(app.pad_source[0].is_none());
    }

    #[test]
    fn minus_equals_adjust_focused_pad_volume() {
        let mut app = App::new();
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.mixer.pad_gain(0), 1.0);
        assert_eq!(app.on_key(key('-')), Action::Mark);
        assert!(app.mixer.pad_gain(0) < 1.0);
        assert_eq!(app.on_key(key('=')), Action::Mark);
    }

    #[test]
    fn arrows_volume_and_left_right_move_focus() {
        let down = || KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up = || KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let right = || KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        let mut app = App::new();
        app.set_focus(Focus::Pad(0));
        // Up/Down change the focused pad's volume.
        assert_eq!(app.on_key(down()), Action::Mark);
        assert!(app.mixer.pad_gain(0) < 1.0, "down lowers pad volume");
        app.on_key(up());
        // Left/Right move focus (not volume).
        let before = app.focus_cell();
        app.on_key(right());
        assert_ne!(app.focus_cell(), before, "right moves focus");
        // On the library, Up/Down browse instead of changing volume.
        app.set_focus(Focus::Crate);
        assert_eq!(app.on_key(down()), Action::CrateNav);
    }

    #[test]
    fn semicolon_cycles_the_focused_pad_kind() {
        let mut app = App::new();
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.mixer.pad_kind(0), PadKind::OneShot);
        assert_eq!(app.on_key(key(';')), Action::Mark);
        assert_eq!(app.mixer.pad_kind(0), PadKind::Loop);
    }

    #[test]
    fn save_pad_writes_trimmed_clip_new_and_over() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-saveback-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("loop.wav");
        termkrush_core::audio::write_wav(&src, &vec![0.5f32; 2000], 44_100, 2).unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        app.set_focus(Focus::Pad(0));
        app.mixer.assign_pad(0, vec![0.5; 2000]); // 1000 frames
        app.pad_source[0] = Some(src.clone());
        app.mixer.nudge_pad_out(0, -10_000); // trim it down
                                             // Save as new → a -edit file appears.
        assert_eq!(app.on_key(key('S')), Action::Record);
        assert!(tmp.join("loop-edit1.wav").exists());
        // Overwrite the source with the (smaller) trimmed clip.
        let before = fs::metadata(&src).unwrap().len();
        assert_eq!(app.on_key(key('O')), Action::Record);
        let after = fs::metadata(&src).unwrap().len();
        assert!(after < before, "source overwritten with the shorter trim");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn render_arrangement_produces_audio_of_the_right_length() {
        let mut app = App::new();
        app.mixer.set_sample_rate(1000);
        app.mixer.set_master_bpm(Some(120.0)); // 125 frames/step
        app.mixer.assign_pad(0, vec![0.5; 8000]);
        app.timeline.set_step(0, 0, true);
        let out = app.render_arrangement();
        // 64 steps × 125 frames = 8000 frames → 16000 interleaved samples.
        assert!(
            (out.len() as i64 - 16_000).abs() < 1100,
            "len {}",
            out.len()
        );
        assert!(
            out.iter().any(|&s| s.abs() > 0.01),
            "rendered audio is non-silent"
        );
        assert!(!app.playing, "transport restored after render");
    }

    #[test]
    fn w_renders_a_wav_into_the_library() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-render-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        app.mixer.set_master_bpm(Some(120.0));
        app.mixer.assign_pad(0, vec![0.5; 8000]);
        app.timeline.set_step(0, 0, true);
        app.on_key(key('t')); // open editor
        assert_eq!(app.on_key(key('w')), Action::Record); // render+save
        assert!(
            tmp.join("mix-1.wav").exists(),
            "rendered WAV in the library"
        );
        // It reappears in the listing.
        assert!(app.filtered().iter().any(|(n, _, _)| n == "mix-1.wav"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn space_toggles_global_pause() {
        let mut app = App::new();
        assert!(!app.mixer.is_paused());
        assert_eq!(app.on_key(key(' ')), Action::Mark);
        assert!(app.mixer.is_paused(), "space pauses the whole mix");
        app.on_key(key(' '));
        assert!(!app.mixer.is_paused(), "space resumes");
    }

    #[test]
    fn arrangement_transport_pauses_and_resumes_from_position() {
        let mut app = App::new();
        app.mixer.set_sample_rate(1000);
        app.mixer.set_master_bpm(Some(120.0)); // 125 frames/step
        app.timeline.set_step(0, 0, true);
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.toggle_transport(); // p — play
        assert!(app.playing);
        app.advance_playback(400);
        let pos = app.play_step;
        assert!(pos > 0, "advanced");
        app.toggle_transport(); // pause
        assert!(!app.playing);
        app.advance_playback(2000);
        assert_eq!(app.play_step, pos, "paused holds position");
        app.toggle_transport(); // resume
        assert_eq!(app.play_step, pos, "resumes from where it paused");
        app.stop_transport();
        assert!(!app.playing && app.play_step == 0, "stop rewinds to top");
    }

    #[test]
    fn transport_fires_pads_on_their_steps() {
        let mut app = App::new();
        app.mixer.set_sample_rate(1000);
        app.mixer.set_master_bpm(Some(120.0)); // beat 500f, 16ths → 125f/step
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.timeline.set_step(0, 0, true);
        app.toggle_transport(); // play (fires step 0)
        assert!(app.playing);
        app.advance_playback(1); // step 0 is due immediately on start
        assert_eq!(app.mixer.active_voices(), 1, "pad fired on its step");
        app.toggle_transport();
        assert!(!app.playing);
    }

    #[test]
    fn v_draws_a_loop_region_across_steps() {
        let mut app = App::new();
        app.on_key(key('t')); // open editor (lane 0, step 0)
        assert_eq!(app.on_key(key('v')), Action::Timeline); // mark start at 0
        assert!(app.tl_region_start.is_some());
        for _ in 0..4 {
            app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        }
        app.on_key(key('v')); // fill 0..=4 on lane 0
        assert!(app.tl_region_start.is_none());
        for s in 0..=4 {
            assert!(app.timeline.step(0, s), "region filled at {s}");
        }
        assert_eq!(app.timeline.run_at(0, 2), Some((0, 5)));
    }

    #[test]
    fn esc_leaves_the_timeline_then_opens_quit() {
        let esc = || KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut app = App::new();
        app.on_key(key('t'));
        assert!(app.tl_visible);
        app.on_key(esc());
        assert!(!app.tl_visible, "first esc closes the timeline");
        assert!(!app.confirm_quit, "and does not open quit yet");
        app.on_key(esc());
        assert!(app.confirm_quit, "second esc opens the quit modal");
    }

    #[test]
    fn esc_leaves_the_clip_editor_then_opens_quit() {
        let esc = || KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.set_focus(Focus::Pad(0));
        app.on_key(key('e'));
        assert!(app.clip_edit.is_some());
        app.on_key(esc());
        assert!(app.clip_edit.is_none(), "first esc closes the clip editor");
        assert!(!app.confirm_quit);
        app.on_key(esc());
        assert!(app.confirm_quit, "second esc opens quit");
    }

    #[test]
    fn t_opens_timeline_and_cursor_toggles_a_step() {
        let mut app = App::new();
        assert!(!app.tl_visible);
        assert_eq!(app.on_key(key('t')), Action::Timeline);
        assert!(app.tl_visible);
        // Move the cursor and toggle a step.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // lane 1
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)); // step 1
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Action::Timeline
        );
        assert!(app.timeline.step(1, 1), "step placed at the cursor");
        assert_eq!(app.timeline.pads_at(1), vec![1]);
        // Render shows the grid; t closes it.
        assert!(buffer_text(&render(&app, 96, 24)).contains("Timeline"));
        assert_eq!(app.on_key(key('t')), Action::Timeline);
        assert!(!app.tl_visible);
    }

    #[test]
    fn capital_c_clears_the_scratch_phrase() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 8000]);
        app.set_focus(Focus::Pad(0));
        app.mixer.cycle_pad_kind(0);
        app.mixer.cycle_pad_kind(0); // → Scratch
        app.mixer.push_phrase(0, ScratchUnit::Wiki);
        app.mixer.push_phrase(0, ScratchUnit::Whip);
        assert_eq!(app.mixer.pad_phrase_glyphs(0), "><");
        assert_eq!(app.on_key(key('C')), Action::Mark);
        assert_eq!(app.mixer.pad_phrase_len(0), 0);
    }

    #[test]
    fn p_records_a_scratch_phrase_by_tapping() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 8000]);
        app.set_focus(Focus::Pad(0));
        app.mixer.cycle_pad_kind(0);
        app.mixer.cycle_pad_kind(0); // → Scratch
        assert_eq!(app.on_key(key('P')), Action::Mark); // arm phrase record
        assert_eq!(app.phrase_rec, Some(0));
        app.on_key(key('j')); // tap wiki
        app.on_key(key('k')); // tap whip
        app.on_key(key('j')); // tap wiki
        assert_eq!(app.mixer.pad_phrase_len(0), 3);
        app.on_key(key('P')); // stop recording
        assert_eq!(app.phrase_rec, None);
        // Now j plays the stored phrase as one voice.
        let before = app.mixer.active_voices();
        app.on_key(key('j'));
        assert_eq!(app.mixer.active_voices(), before + 1);
    }

    #[test]
    fn scratch_pad_j_and_k_play_wiki_and_whip() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 4000]);
        app.set_focus(Focus::Pad(0));
        app.mixer.cycle_pad_kind(0); // OneShot → Loop
        app.mixer.cycle_pad_kind(0); // → Scratch
        assert_eq!(app.mixer.pad_kind(0), PadKind::Scratch);
        assert_eq!(app.on_key(key('j')), Action::TriggerPad); // wiki
        assert_eq!(app.mixer.active_voices(), 1);
        assert_eq!(app.on_key(key('k')), Action::TriggerPad); // whip
        assert_eq!(app.mixer.active_voices(), 2);
    }

    #[test]
    fn pad_trigger_plays_an_assigned_clip() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.on_key(key('j')), Action::TriggerPad);
        assert_eq!(app.mixer.active_voices(), 1);
        // A number key triggers directly too.
        assert_eq!(app.on_key(key('1')), Action::TriggerPad);
        assert_eq!(app.mixer.active_voices(), 2);
    }

    #[test]
    fn pad_trim_keys_move_bounds_nondestructively() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 20_000]);
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.mixer.pad_trim(0), (0, 10_000));
        assert_eq!(app.on_key(key('d')), Action::Mark); // in-point forward
        assert!(app.mixer.pad_trim(0).0 > 0);
        assert_eq!(app.on_key(key('s')), Action::Mark); // out-point in
        assert!(app.mixer.pad_trim(0).1 < 10_000);
        assert_eq!(app.mixer.pad_clip_frames(0), 10_000, "source untouched");
    }

    #[test]
    fn r_records_the_live_mix_into_the_stash() {
        let mut app = App::new();
        app.set_focus(Focus::Crate); // not a pad → goes to the stash
        app.mixer.assign_pad(0, vec![0.4; 4096]);
        app.mixer.trigger_pad(0);
        assert_eq!(app.on_key(key('r')), Action::Record); // arm
        assert!(app.mixer.is_recording());
        app.mixer.fill_mix(&mut [0.0f32; 256]);
        assert_eq!(app.on_key(key('r')), Action::Record); // disarm → stash
        assert!(!app.mixer.is_recording());
        assert_eq!(app.recordings.len(), 1);
    }

    #[test]
    fn master_volume_keys_adjust_gain() {
        let mut app = App::new();
        let g0 = app.mixer.master_gain();
        assert_eq!(app.on_key(key(']')), Action::MasterGain);
        assert!(app.mixer.master_gain() > g0);
        assert_eq!(app.on_key(key('[')), Action::MasterGain);
    }

    #[test]
    fn esc_opens_quit_modal_then_y_quits() {
        let mut app = App::new();
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Action::ConfirmQuit
        );
        assert!(app.confirm_quit && !app.should_quit);
        assert_eq!(app.on_key(key('n')), Action::None);
        assert!(!app.confirm_quit);
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.on_key(key('y')), Action::Quit);
        assert!(app.should_quit);
    }

    /// Walks a realistic session end-to-end through the public key API and
    /// asserts it neither panics nor goes silent — the cross-feature coverage
    /// piecemeal unit tests miss (where "no pause / no unload / unusable trim"
    /// would have shown up).
    #[test]
    fn full_session_flow_does_not_panic_and_renders() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-e2e-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        app.mixer.set_sample_rate(1000);
        app.mixer.assign_pad(0, vec![0.5; 4000]);
        app.mixer.assign_pad(1, vec![0.4; 4000]);
        app.mixer.set_pad_bpm(0, Some(120.0));
        app.mixer.set_master_bpm(Some(120.0));

        // Pad 0 → loop; pad 1 → clip-edit (trim + cut) then back.
        app.set_focus(Focus::Pad(0));
        app.on_key(key(';'));
        assert_eq!(app.mixer.pad_kind(0), PadKind::Loop);
        app.set_focus(Focus::Pad(1));
        app.on_key(key('e')); // clip-edit
        for _ in 0..3 {
            app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)); // move in-mark
        }
        app.on_key(key(' ')); // audition
        app.on_key(key('x')); // snip both sides
        app.on_key(key('e')); // close
        assert!(app.clip_edit.is_none());

        // Arrange: place a hit, play (p), pause (p), render the mix.
        app.on_key(key('t'));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // lane 0, step 0
        app.on_key(key('p')); // play the arrangement
        app.advance_playback(600);
        app.on_key(key('p')); // pause
        let mix = app.render_arrangement();
        assert!(
            mix.iter().any(|&s| s.abs() > 0.01),
            "arrangement renders audio"
        );
        app.on_key(key('t')); // close timeline

        // Export pad 1 to mp3, then unload pad 0.
        app.set_focus(Focus::Pad(1));
        assert_eq!(app.on_key(key('E')), Action::Record);
        assert!(
            fs::read_dir(&tmp).unwrap().any(|e| e
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "mp3")),
            "mp3 exported"
        );
        app.set_focus(Focus::Pad(0));
        app.on_key(key('u'));
        assert!(!app.mixer.pad_loaded(0), "unloaded");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn first_track_sets_master_later_tracks_adopt_silently() {
        use termkrush_core::audio::DecodedAudio;
        fn load(app: &mut App, pad: usize, bpm: f32) {
            app.place_decoded(Decoded {
                target: LoadTarget::Pad(pad),
                track: DecodedAudio {
                    samples: vec![0.5; 64],
                    sample_rate: 44_100,
                    channels: 2,
                    source_sample_rate: 44_100,
                    source_channels: 2,
                    duration_secs: 0.0,
                    title: None,
                    artist: None,
                },
                path: format!("/m/{pad}.mp3").into(),
                bpm: Some(bpm),
            });
        }
        let mut app = App::new();
        // First track silently sets the master tempo.
        load(&mut app, 0, 120.0);
        assert_eq!(app.mixer.master_bpm(), Some(120.0));
        // A later off-tempo track adopts the master — no prompt, master unchanged.
        load(&mut app, 1, 140.0);
        assert_eq!(
            app.mixer.master_bpm(),
            Some(120.0),
            "later tracks adopt, no prompt"
        );
    }

    #[test]
    fn renders_timeline_strip_wordmark_and_eight_pads() {
        let app = App::new();
        let text = buffer_text(&render(&app, 96, 40));
        assert!(text.contains("TermKrush"));
        assert!(
            text.contains("TIMELINE"),
            "persistent timeline strip on top"
        );
        assert!(text.contains("Pad 1"));
        assert!(text.contains("Pad 8"), "eighth pad renders");
        assert!(!text.contains("=^.^="), "no DJ cat");
    }
}
