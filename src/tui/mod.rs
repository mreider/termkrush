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

/// Clip-editor zoom levels — the window span. Index 0 = the whole clip.
const CE_ZOOM_SECS: [f64; 5] = [0.0, 10.0, 1.0, 0.1, 0.01];
const CE_ZOOM_LABELS: [&str; 5] = ["whole", "10s", "1s", "100ms", "10ms"];

/// `(lo, hi)` frame bounds of a `span`-wide window centered on `center`,
/// clamped inside `0..len`.
fn window_bounds(center: usize, span: usize, len: usize) -> (usize, usize) {
    let span = span.clamp(1, len.max(1));
    let half = span / 2;
    let lo = center.saturating_sub(half);
    let hi = (lo + span).min(len);
    (hi.saturating_sub(span), hi)
}

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
    Focus,
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
    Timeline,
}

/// An action a context-menu item runs (routes to existing handlers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    // Pad
    Load,
    EditClip,
    CycleKind,
    ToggleActive,
    SaveNew,
    SaveOver,
    Export,
    Unload,
    PhraseRec,
    ClearPhrase,
    // Timeline
    PlaceHit,
    Record,
    Cut,
    Region,
    Clear,
    Render,
    TempoUp,
    TempoDown,
    MasterUp,
    MasterDown,
}

/// An open context menu: a titled list navigated with arrows, run with Enter.
struct MenuState {
    title: &'static str,
    items: Vec<(&'static str, MenuAction)>,
    sel: usize,
}

/// The "move a file" modal: pick a destination folder (one level under root),
/// with inline new/rename/delete.
struct MovePicker {
    file: PathBuf,                             // the file being moved
    folders: Vec<(String, PathBuf)>,           // root subfolders; index + 1 (0 = root)
    sel: usize,                                // 0 = (root), else folders[sel - 1]
    naming: Option<(String, Option<PathBuf>)>, // (buffer, Some=rename target / None=new)
    confirm_del: Option<PathBuf>,              // folder pending a delete confirm
}

/// State of the keyboard-first pads UI.
pub struct App {
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
    /// The scratch pad currently recording a phrase (taps append to it).
    phrase_rec: Option<usize>,
    /// Source file each pad was loaded from (for save-over).
    pad_source: [Option<PathBuf>; PADS],
    /// The arrangement grid (edited in-place via the focused Timeline strip).
    timeline: Timeline,
    tl_lane: usize,
    tl_step: usize,
    /// First endpoint of a loop region being drawn (`v` marks, `v` fills).
    tl_region_start: Option<usize>,
    /// Open context menu (Enter on a pad/timeline), if any.
    menu: Option<MenuState>,
    /// A library song is selected for action (load/rename/delete/move).
    song_action: bool,
    /// Whether a library file is actively highlighted (arrows highlight, esc
    /// clears). When highlighted, 1-8 loads; otherwise 1-8 jumps to a pad.
    crate_active: bool,
    /// Pending overwrite confirm: load `file` onto already-loaded pad `i`.
    confirm_override: Option<(usize, PathBuf)>,
    /// The move-a-file modal, if open.
    move_picker: Option<MovePicker>,
    /// Clip-edit modal: the pad being edited + which mark is active
    /// (`false` = in/left, `true` = out/right) + the zoom level index.
    clip_edit: Option<usize>,
    ce_out: bool,
    ce_zoom: usize,
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
            should_quit: false,
            confirm_quit: false,
            mixer: Mixer::new(),
            focus: Focus::Crate, // the library is home
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
            phrase_rec: None,
            pad_source: std::array::from_fn(|_| None),
            timeline: Timeline::default(),
            tl_lane: 0,
            tl_step: 0,
            tl_region_start: None,
            menu: None,
            song_action: false,
            crate_active: false,
            confirm_override: None,
            move_picker: None,
            clip_edit: None,
            ce_out: false,
            ce_zoom: 0,
            playing: false,
            play_acc: 0.0,
            play_step: 0,
            prev_run: [false; PADS],
        }
    }

    /// Play / **pause** the timeline transport. Pausing stops sound and holds
    /// the position; resuming continues from there (not the top). Starting
    /// clears live pad voices so the arrangement plays alone.
    fn toggle_transport(&mut self) -> Action {
        if self.playing {
            self.playing = false;
            self.silence_pads(); // pause = go quiet, keep play_step
        } else {
            self.mixer.clear_voices(); // exclusive with live pad play
            self.playing = true;
            self.prev_run = [false; PADS]; // re-fire the current step's pads
            self.play_acc = self.frames_per_step(); // fire promptly
        }
        Action::Timeline
    }

    /// Fade every pad out (used on pause so the mix goes quiet).
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

    /// Arrow keys — context navigation: browse the library, adjust a pad's
    /// volume, or move the timeline cursor. (Pads/timeline are *selected* by
    /// number, not arrows.)
    fn nav(&mut self, dir: KeyCode) -> Action {
        match self.focus {
            Focus::Crate => match dir {
                // First arrow highlights the current row; further arrows move.
                KeyCode::Up if !self.crate_active => {
                    self.crate_active = true;
                    Action::CrateNav
                }
                KeyCode::Down if !self.crate_active => {
                    self.crate_active = true;
                    Action::CrateNav
                }
                KeyCode::Up => self.crate_nav(-1),
                KeyCode::Down => self.crate_nav(1),
                _ => Action::None,
            },
            Focus::Pad(_) => match dir {
                KeyCode::Up => self.volume(true),
                KeyCode::Down => self.volume(false),
                _ => Action::None,
            },
            Focus::Timeline => {
                let last = self.timeline.total_steps().saturating_sub(1);
                match dir {
                    KeyCode::Left => self.tl_step = self.tl_step.saturating_sub(1),
                    KeyCode::Right => self.tl_step = (self.tl_step + 1).min(last),
                    KeyCode::Up => self.tl_lane = self.tl_lane.saturating_sub(1),
                    KeyCode::Down => self.tl_lane = (self.tl_lane + 1).min(PADS - 1),
                    _ => {}
                }
                Action::Timeline
            }
        }
    }

    /// The context hint shown under the title — what the keys do right now,
    /// so the UI teaches itself and needs no help screen.
    fn hint(&self) -> &'static str {
        if self.menu.is_some() {
            return "↑↓ choose · enter do · esc close";
        }
        if self.clip_edit.is_some() {
            return "←→ move · +/- zoom · tab in/out (follows) · space play · enter snip · esc done";
        }
        match self.focus {
            Focus::Crate if self.crate_active => {
                "1-8 load · r rename · ⌫ del · move · esc unselect"
            }
            Focus::Crate => "↑↓ highlight a song · 1-8 jump to pad · 0 timeline · esc quit",
            Focus::Pad(_) => "↑↓ vol · space play · enter menu · 1-8/0 switch · esc library",
            Focus::Timeline => "←→ beat · ↑↓ lane · space play · enter menu · esc library",
        }
    }

    /// Space — play the focused pad, or play/stop the arrangement. Triggering a
    /// pad stops the timeline first, so live play and the arrangement never
    /// sound at once.
    fn play(&mut self) -> Action {
        match self.focus {
            Focus::Pad(i) => {
                if self.playing {
                    self.playing = false;
                    self.mixer.clear_voices();
                }
                self.trigger(i)
            }
            Focus::Timeline => self.toggle_transport(),
            Focus::Crate => Action::None,
        }
    }

    /// Enter — open the context menu, except while recording a scratch phrase,
    /// where Enter taps a whip (Space taps a wiki).
    fn enter_action(&mut self) -> Action {
        if self.phrase_rec.is_some() {
            return self.secondary(); // whip tap
        }
        match self.focus {
            // Library: open a folder, or offer actions on a song.
            Focus::Crate => {
                self.crate_active = true; // Enter highlights too
                if self.selected_is_dir() {
                    self.enter_selected()
                } else if self.selected_path().is_some() {
                    self.song_action = true;
                    Action::Mark
                } else {
                    Action::None
                }
            }
            _ => self.open_menu(),
        }
    }

    /// Keys while a library song's action prompt is up: `1`–`8` load it onto
    /// that pad, `r`/Insert rename, Backspace/Delete delete, `→` move, Esc cancel.
    fn on_song_action_key(&mut self, key: KeyEvent) -> Action {
        self.song_action = false;
        // Move direction follows the folder model: at root → `→` (into a
        // folder); inside a folder → `←` (out toward root).
        let in_folder = self.in_subfolder();
        match key.code {
            KeyCode::Char(c @ '1'..='8') => {
                let pad = c as usize - '1' as usize;
                // Loading onto an already-loaded pad asks to confirm first.
                if self.mixer.pad_loaded(pad) {
                    if let Some(file) = self.selected_path() {
                        self.confirm_override = Some((pad, file));
                    }
                    Action::Mark
                } else {
                    self.load_selected_onto(pad)
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Insert => self.start_rename(),
            KeyCode::Backspace | KeyCode::Delete => self.arm_delete(),
            KeyCode::Right if !in_folder => self.open_move_picker(),
            KeyCode::Left if in_folder => self.open_move_picker(),
            _ => Action::Mark, // Esc / anything else cancels
        }
    }

    /// Whether the library is currently showing a subfolder (not the root).
    fn in_subfolder(&self) -> bool {
        self.crate_lib.cwd() != self.crate_lib.root()
    }

    /// Open the move-a-file modal for the highlighted song.
    fn open_move_picker(&mut self) -> Action {
        if let Some(file) = self.selected_path() {
            self.move_picker = Some(MovePicker {
                file,
                folders: self.crate_lib.root_folders(),
                sel: 0,
                naming: None,
                confirm_del: None,
            });
            Action::Mark
        } else {
            Action::None
        }
    }

    /// Keys while the move modal is open.
    fn on_move_key(&mut self, key: KeyEvent) -> Action {
        let Some(mp) = self.move_picker.as_mut() else {
            return Action::None;
        };
        // Name-entry sub-mode (new folder or rename).
        if let Some((buf, target)) = mp.naming.as_mut() {
            match key.code {
                KeyCode::Char(c) => buf.push(c),
                KeyCode::Backspace => {
                    buf.pop();
                }
                KeyCode::Esc => mp.naming = None,
                KeyCode::Enter => {
                    let (name, target) = (buf.clone(), target.clone());
                    mp.naming = None;
                    if name.trim().is_empty() {
                        return Action::Mark;
                    }
                    match target {
                        Some(path) => {
                            let _ = self.crate_lib.rename(&path, name.trim());
                        }
                        None => {
                            let _ = self.crate_lib.make_folder(name.trim());
                        }
                    }
                    if let Some(mp) = self.move_picker.as_mut() {
                        mp.folders = self.crate_lib.root_folders();
                        mp.sel = mp.sel.min(mp.folders.len());
                    }
                }
                _ => {}
            }
            return Action::Mark;
        }
        // Delete-confirm sub-mode.
        if let Some(path) = mp.confirm_del.clone() {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                let _ = self.crate_lib.delete_folder(&path);
                if let Some(mp) = self.move_picker.as_mut() {
                    mp.folders = self.crate_lib.root_folders();
                    mp.sel = mp.sel.min(mp.folders.len());
                }
            }
            if let Some(mp) = self.move_picker.as_mut() {
                mp.confirm_del = None;
            }
            return Action::Mark;
        }
        // Browsing.
        let n = mp.folders.len();
        match key.code {
            KeyCode::Up => mp.sel = mp.sel.saturating_sub(1),
            KeyCode::Down => mp.sel = (mp.sel + 1).min(n),
            KeyCode::Char('n') | KeyCode::Char('N') => mp.naming = Some((String::new(), None)),
            KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Insert if mp.sel > 0 => {
                let (name, path) = mp.folders[mp.sel - 1].clone();
                mp.naming = Some((name, Some(path)));
            }
            KeyCode::Backspace | KeyCode::Delete if mp.sel > 0 => {
                mp.confirm_del = Some(mp.folders[mp.sel - 1].1.clone());
            }
            KeyCode::Esc | KeyCode::Left => {
                self.move_picker = None;
            }
            KeyCode::Enter => {
                // sel 0 = root, else the chosen folder.
                let dest = if mp.sel == 0 {
                    self.crate_lib.root().to_path_buf()
                } else {
                    mp.folders[mp.sel - 1].1.clone()
                };
                let file = mp.file.clone();
                let _ = self.crate_lib.move_into(&file, &dest);
                self.move_picker = None;
                self.crate_sel = 0;
            }
            _ => {}
        }
        Action::Mark
    }

    /// Toggle the timeline loop region (first call marks the start, second
    /// fills to the cursor).
    fn toggle_region(&mut self) -> Action {
        match self.tl_region_start {
            None => self.tl_region_start = Some(self.tl_step),
            Some(s) => {
                self.timeline.fill_region(self.tl_lane, s, self.tl_step);
                self.tl_region_start = None;
            }
        }
        Action::Timeline
    }

    /// Enter — open the context menu for the focused area. The first item is
    /// the common action, so Enter-then-Enter does the obvious thing.
    fn open_menu(&mut self) -> Action {
        use MenuAction::*;
        let (title, items): (&str, Vec<(&str, MenuAction)>) = match self.focus {
            // The library has no menu — Enter offers song actions directly.
            Focus::Crate => return Action::None,
            Focus::Pad(i) => {
                let mut items: Vec<(&str, MenuAction)> = Vec::new();
                if self.mixer.pad_loaded(i) {
                    items.push(("edit clip", EditClip));
                }
                items.push(("load", Load));
                if self.mixer.pad_loaded(i) {
                    items.extend([
                        ("kind", CycleKind),
                        ("on/off", ToggleActive),
                        ("save", SaveNew),
                        ("save over", SaveOver),
                        ("export mp3", Export),
                        ("unload", Unload),
                        ("rec phrase", PhraseRec),
                        ("clear phrase", ClearPhrase),
                    ]);
                }
                ("Pad", items)
            }
            Focus::Timeline => (
                "Timeline",
                vec![
                    ("place pad", PlaceHit),
                    ("record", Record),
                    ("cut", Cut),
                    ("region", Region),
                    ("clear", Clear),
                    ("render", Render),
                    ("tempo +", TempoUp),
                    ("tempo -", TempoDown),
                    ("master +", MasterUp),
                    ("master -", MasterDown),
                ],
            ),
        };
        self.menu = Some(MenuState {
            title,
            items,
            sel: 0,
        });
        Action::Mark
    }

    /// Run a chosen menu action (closes the menu first).
    fn run_menu(&mut self, a: MenuAction) -> Action {
        use MenuAction::*;
        self.menu = None;
        match a {
            EditClip => self.open_clip_edit(),
            PlaceHit => {
                self.timeline.toggle(self.tl_lane, self.tl_step);
                Action::Timeline
            }
            Load => self.load_selected_onto(self.active_pad()),
            CycleKind => self.cycle_kind(),
            ToggleActive => self.toggle_active(),
            SaveNew => self.save_pad_new(),
            SaveOver => self.save_pad_over(),
            Export => self.export_pad_mp3(),
            Unload => self.unload(),
            PhraseRec => self.toggle_phrase_rec(),
            ClearPhrase => self.clear_phrase(),
            Record => self.toggle_record(),
            Cut => {
                self.timeline.cut_at(self.tl_step);
                self.tl_step = self
                    .tl_step
                    .min(self.timeline.total_steps().saturating_sub(1));
                Action::Timeline
            }
            Region => self.toggle_region(),
            Clear => {
                self.timeline.clear();
                Action::Timeline
            }
            Render => self.render_to_library(),
            TempoUp => {
                self.mixer.nudge_global_speed(0.02);
                Action::MasterGain
            }
            TempoDown => {
                self.mixer.nudge_global_speed(-0.02);
                Action::MasterGain
            }
            MasterUp => {
                self.mixer.nudge_master(GAIN_NUDGE);
                Action::MasterGain
            }
            MasterDown => {
                self.mixer.nudge_master(-GAIN_NUDGE);
                Action::MasterGain
            }
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

    fn set_focus(&mut self, f: Focus) {
        if let Focus::Pad(i) = f {
            self.last_pad = i;
        }
        // Returning to the library starts with nothing highlighted (arrows
        // begin a highlight again).
        if f == Focus::Crate {
            self.crate_active = false;
        }
        self.focus = f;
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
                self.ce_zoom = 0; // whole clip
                return Action::Mark;
            }
        }
        Action::None
    }

    /// Frames the clip-edit window spans at the current zoom (whole clip at
    /// zoom 0).
    fn ce_window_frames(&self, len: usize) -> usize {
        if self.ce_zoom == 0 {
            return len.max(1);
        }
        let s = CE_ZOOM_SECS[self.ce_zoom.min(CE_ZOOM_SECS.len() - 1)];
        ((s * self.mixer.sample_rate() as f64) as usize).clamp(1, len.max(1))
    }

    /// Handle a key while the clip-edit modal is open. `Tab` switches the
    /// active mark (and the window follows it), `←/→` nudge it by ~one column
    /// at the current zoom, `+`/`-` zoom, `space` auditions, `enter` snips,
    /// `esc` closes.
    fn on_clip_key(&mut self, key: KeyEvent) -> Option<Action> {
        let i = self.clip_edit?;
        let len = self.mixer.pad_clip_frames(i).max(1);
        // One press ≈ one column of the current window, so precision = zoom.
        let step = (self.ce_window_frames(len) / 64).max(1) as i64;
        let (inp, out) = self.mixer.pad_trim(i);
        let nudge = |v: usize, d: i64| (v as i64 + d).max(0) as usize;
        match key.code {
            KeyCode::Esc => {
                self.clip_edit = None;
                Some(Action::Mark)
            }
            KeyCode::Tab | KeyCode::Up | KeyCode::Down => {
                self.ce_out = !self.ce_out; // switch active mark (window follows)
                Some(Action::Mark)
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.ce_zoom = (self.ce_zoom + 1).min(CE_ZOOM_SECS.len() - 1);
                Some(Action::Mark)
            }
            KeyCode::Char('-') => {
                self.ce_zoom = self.ce_zoom.saturating_sub(1);
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
                // Toggle the audition. Play ~1.5s AT the active handle so you
                // hear the exact edit point: the in-handle plays forward from
                // in; the out-handle plays the run-up to out.
                if self.mixer.pad_is_sounding(i) {
                    self.mixer.stop_pad(i);
                } else {
                    let prev = (self.mixer.sample_rate() as usize * 3 / 2).max(1); // ~1.5s
                    let (from, to) = if self.ce_out {
                        (out.saturating_sub(prev).max(inp), out)
                    } else {
                        (inp, (inp + prev).min(out))
                    };
                    self.mixer.audition_region(i, from, to);
                }
                Some(Action::Mark)
            }
            KeyCode::Enter => {
                self.mixer.snip_pad(i); // destructively keep only [in, out]
                self.ce_out = false;
                Some(Action::Mark)
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

    /// Up/Down on a focused pad → its volume. (Master volume is in the
    /// Timeline menu, since ↑/↓ there move between lanes.)
    fn volume(&mut self, up: bool) -> Action {
        if let Focus::Pad(i) = self.focus {
            self.mixer.nudge_pad_gain(i, if up { 0.05 } else { -0.05 });
        }
        Action::Mark
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

        // Context menu (M) captures input while open.
        if let Some(m) = self.menu.as_mut() {
            return match key.code {
                KeyCode::Up => {
                    m.sel = m.sel.saturating_sub(1);
                    Action::Mark
                }
                KeyCode::Down => {
                    m.sel = (m.sel + 1).min(m.items.len().saturating_sub(1));
                    Action::Mark
                }
                KeyCode::Enter => {
                    let a = m.items[m.sel].1;
                    self.run_menu(a)
                }
                KeyCode::Esc => {
                    self.menu = None;
                    Action::Mark
                }
                _ => Action::None,
            };
        }

        // Overwrite-confirm: y loads over the existing pad, anything else cancels.
        if let Some((pad, file)) = self.confirm_override.clone() {
            self.confirm_override = None;
            return match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.pending_pad_load = Some((pad, file));
                    Action::AssignPad
                }
                _ => Action::Mark,
            };
        }
        // Move-a-file modal captures keys while open.
        if self.move_picker.is_some() {
            return self.on_move_key(key);
        }
        // Library song-action prompt captures keys while up.
        if self.song_action {
            return self.on_song_action_key(key);
        }

        // Clip-edit modal captures keys while open.
        if self.clip_edit.is_some() {
            if let Some(a) = self.on_clip_key(key) {
                return a;
            }
        }

        // In the library on a song, the action keys work directly — no need to
        // press Enter first (Enter just shows the prompt as a reminder).
        if self.focus == Focus::Crate
            && self.crate_active
            && self.selected_path().is_some()
            && !self.selected_is_dir()
            && matches!(
                key.code,
                KeyCode::Char('1'..='8')
                    | KeyCode::Char('r')
                    | KeyCode::Char('R')
                    | KeyCode::Insert
                    | KeyCode::Backspace
                    | KeyCode::Delete
                    | KeyCode::Left
                    | KeyCode::Right
            )
        {
            self.song_action = true; // on_song_action_key clears it
            return self.on_song_action_key(key);
        }

        // Control surface: numbers select · arrows within · Space · Enter · Esc.
        match key.code {
            // Esc peels back one layer: pad/timeline → library; a highlighted
            // song → stop highlighting; a subfolder → its parent; root → quit.
            KeyCode::Esc => {
                if self.focus != Focus::Crate {
                    self.set_focus(Focus::Crate);
                    Action::Focus
                } else if self.crate_active {
                    self.crate_active = false; // stop highlighting
                    Action::Mark
                } else if self.crate_lib.cwd() != self.crate_lib.root() {
                    if let Some(parent) = self.crate_lib.cwd().parent().map(|p| p.to_path_buf()) {
                        self.crate_lib.enter(&parent);
                        self.crate_sel = 0;
                    }
                    Action::Mark
                } else {
                    self.confirm_quit = true;
                    Action::ConfirmQuit
                }
            }
            // 1–8 select a pad, 0 selects the timeline — from anywhere.
            KeyCode::Char('0') => {
                self.focus = Focus::Timeline;
                Action::Focus
            }
            KeyCode::Char(c @ '1'..='8') => {
                self.set_focus(Focus::Pad(c as usize - '1' as usize));
                Action::Focus
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => self.nav(key.code),
            KeyCode::Char(' ') => self.play(),
            KeyCode::Enter => self.enter_action(),
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

/// The clip-edit modal: a fixed-width bar showing a zoomable WINDOW of the
/// clip that follows the active handle, a full-clip minimap, and a readout.
fn draw_clip_edit(f: &mut Frame, area: Rect, app: &App, pad: usize) {
    let len = app.mixer.pad_clip_frames(pad).max(1);
    let (inp, out) = app.mixer.pad_trim(pad);
    let rate = app.mixer.sample_rate().max(1) as f64;
    let secs = |fr: usize| fr as f64 / rate;
    let active = if app.ce_out { "OUT ▸" } else { "◀ IN" };
    let zoom = CE_ZOOM_LABELS[app.ce_zoom.min(CE_ZOOM_LABELS.len() - 1)];
    let title = format!(
        "Edit Pad {} — moving {active}  ·  zoom {zoom}  (+/-)",
        pad + 1
    );
    let block = cell_block(&title, true);
    let inner = block.inner(area);
    let w = (inner.width as usize).saturating_sub(6).clamp(8, 72);

    // The window of the clip we're looking at, centered on the active handle.
    let span = app.ce_window_frames(len);
    let center = if app.ce_out { out } else { inp };
    let (lo, hi) = window_bounds(center, span, len);
    let win = (hi - lo).max(1);
    // A frame→column mapper over an arbitrary range.
    let map = |fr: usize, a: usize, b: usize| {
        let r = (b - a).max(1);
        ((fr.saturating_sub(a)) * w / r).min(w - 1)
    };

    // Main bar: the window. Filled where inside [in, out); handles where visible.
    let mut bar: Vec<char> = (0..w)
        .map(|c| {
            let fr = lo + c * win / w;
            if fr >= inp && fr < out {
                '█'
            } else {
                '░'
            }
        })
        .collect();
    if (lo..hi).contains(&inp) {
        bar[map(inp, lo, hi)] = if app.ce_out { '◂' } else { '◀' };
    }
    if out > 0 && (lo..=hi).contains(&(out - 1).max(lo)) && out > lo {
        bar[map(out.saturating_sub(1).max(lo), lo, hi)] = if app.ce_out { '▶' } else { '▸' };
    }

    // Minimap: the whole clip, with the visible window marked and both handles.
    let mut mini: Vec<char> = (0..w)
        .map(|c| {
            let fr = c * len / w;
            if fr >= lo && fr < hi {
                '─'
            } else {
                '·'
            }
        })
        .collect();
    mini[map(inp, 0, len)] = '◀';
    mini[map(out.saturating_sub(1), 0, len)] = '▸';

    let lines = vec![
        Line::from(format!("  [{}]", bar.iter().collect::<String>())),
        Line::from(""),
        Line::from(format!(
            "  in {:.3}s   out {:.3}s   selection {:.3}s",
            secs(inp),
            secs(out),
            secs(out.saturating_sub(inp)),
        )),
        Line::from(""),
        Line::from(format!(
            "  whole: {}  ({:.2}s)",
            mini.iter().collect::<String>(),
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
/// The master-timeline strip across the top: one lane per pad, hits as `█`,
/// playhead `▶`/`:`, and — when the Timeline is focused — an edit cursor
/// (`▣`/`▢`). Edited in place; no separate editor.
fn draw_timeline_strip(f: &mut Frame, area: Rect, app: &App) {
    let tl = &app.timeline;
    let spb = tl.steps_per_bar();
    let focused = app.focus_cell() == Focus::Timeline;
    let transport = if app.playing { "▶" } else { "■" };
    let title = format!("0 TIMELINE {transport} {}×{}", tl.bars(), spb);
    let block = cell_block(&title, focused);
    let head = app.playhead();
    let mut lines: Vec<Line> = Vec::with_capacity(PADS);
    for lane in 0..PADS {
        // Label each lane with its pad's track so it's clear lane = pad.
        let name = app.pad_source[lane]
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("—");
        let mark = if focused && lane == app.tl_lane {
            '▸'
        } else {
            ' '
        };
        let mut s = format!("{mark}P{} {name:<8.8} ", lane + 1);
        for step in 0..tl.total_steps() {
            if step > 0 && step % spb == 0 {
                s.push('|'); // bar boundary
            }
            let hit = tl.step(lane, step);
            let cursor = focused && lane == app.tl_lane && step == app.tl_step;
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

fn return_help(f: &mut Frame, app: &App) {
    if app.confirm_quit {
        draw_quit_modal(f, f.area());
    } else if let Some((pad, _)) = &app.confirm_override {
        draw_confirm_modal(
            f,
            f.area(),
            &format!("Pad {} is loaded — overwrite?", pad + 1),
        );
    } else if let Some(path) = &app.confirm_delete {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("track");
        draw_confirm_modal(f, f.area(), &format!("Delete {name}?"));
    } else if let Some((path, buf)) = &app.rename {
        let old = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        draw_rename_modal(f, f.area(), old, buf);
    } else if let Some(mp) = &app.move_picker {
        draw_move_picker(f, f.area(), mp);
    } else if app.song_action {
        let name = app
            .selected_path()
            .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .unwrap_or_default();
        draw_song_action(f, f.area(), &name, app.in_subfolder());
    } else if let Some(menu) = &app.menu {
        draw_menu(f, f.area(), menu);
    }
}

/// The rename modal: the old name in the title, the new-name buffer in the body.
fn draw_rename_modal(f: &mut Frame, area: Rect, old: &str, buf: &str) {
    let w = ((old.len().max(buf.len()) as u16) + 8).clamp(28, 72);
    let popup = centered(w, 4, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(AMBER))
        .title(format!(" rename {old} "));
    let lines = vec![
        Line::from(format!("  {buf}▏")),
        Line::from("  enter save · esc cancel"),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(GREEN)),
        popup,
    );
}

/// The song-action prompt: one command per line, key then what it does.
fn draw_song_action(f: &mut Frame, area: Rect, name: &str, in_folder: bool) {
    // Move arrow follows the folder model: out (←) from a subfolder, in (→) at root.
    let (move_key, move_what) = if in_folder {
        ("←", "move out")
    } else {
        ("→", "move to folder")
    };
    let rows = [
        ("1-8", "load to pad"),
        ("r", "rename"),
        ("⌫", "delete"),
        (move_key, move_what),
        ("esc", "cancel"),
    ];
    let w = (name.len() as u16 + 6).clamp(34, 72);
    let popup = centered(w, rows.len() as u16 + 2, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(AMBER))
        .title(format!(" {name} ")); // the song name IS the title
    let mut lines = Vec::new();
    for (k, what) in rows {
        lines.push(Line::from(format!("  {k:<5}{what}")));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(GREEN)),
        popup,
    );
}

/// The move-a-file modal: the destination folders ((root) + one level),
/// with inline new/rename/delete.
fn draw_move_picker(f: &mut Frame, area: Rect, mp: &MovePicker) {
    let file = mp
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let mut rows: Vec<String> = vec!["(root)".into()];
    rows.extend(mp.folders.iter().map(|(n, _)| format!("📁 {n}")));
    // Commands as key → explanation, one per line, per sub-mode.
    let cmds: &[(&str, &str)] = if mp.naming.is_some() {
        &[("type", "name"), ("enter", "save"), ("esc", "cancel")]
    } else if mp.confirm_del.is_some() {
        &[("y", "delete + contents"), ("n", "keep")]
    } else {
        &[
            ("↑↓", "pick"),
            ("enter", "move here"),
            ("n", "new folder"),
            ("r", "rename"),
            ("⌫", "delete"),
            ("esc", "cancel"),
        ]
    };
    let widest = rows
        .iter()
        .map(|r| r.chars().count())
        .chain(
            cmds.iter()
                .map(|(k, w)| k.chars().count() + w.chars().count() + 3),
        )
        .max()
        .unwrap_or(30) as u16;
    let body_h = if mp.naming.is_some() {
        1
    } else {
        rows.len() as u16
    };
    let popup = centered(
        (widest + 6).clamp(28, 70),
        (body_h + cmds.len() as u16 + 4).min(24),
        area,
    );
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(AMBER))
        .title(format!(" Move {file} "));
    let mut lines: Vec<Line> = Vec::new();
    if let Some((buf, target)) = &mp.naming {
        let what = if target.is_some() {
            "rename to"
        } else {
            "new folder"
        };
        lines.push(Line::from(format!("  {what}: {buf}▏")));
    } else {
        for (i, r) in rows.iter().enumerate() {
            let marker = if i == mp.sel { "▶ " } else { "  " };
            lines.push(Line::from(format!("{marker}{r}")));
        }
    }
    lines.push(Line::from(""));
    for (k, what) in cmds {
        lines.push(Line::from(format!("  {k:<6}{what}")));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(GREEN)),
        popup,
    );
}

/// The context menu (M): a titled list, the selected row reversed.
fn draw_menu(f: &mut Frame, area: Rect, menu: &MenuState) {
    let w = menu
        .items
        .iter()
        .map(|(l, _)| l.chars().count())
        .chain(std::iter::once(menu.title.chars().count()))
        .max()
        .unwrap_or(20) as u16
        + 6;
    let h = menu.items.len() as u16 + 2;
    let popup = centered(w, h, area);
    f.render_widget(Clear, popup);
    let items: Vec<ListItem> = menu
        .items
        .iter()
        .map(|(label, _)| ListItem::new(*label))
        .collect();
    let mut state = ListState::default();
    state.select(Some(menu.sel.min(menu.items.len().saturating_sub(1))));
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(AMBER))
                .title(format!(" {} ", menu.title)),
        )
        .style(Style::default().fg(GREEN))
        .highlight_style(Style::default().fg(AMBER).add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, popup, &mut state);
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
        // Context hint — what the keys do *right now*. No help screen needed.
        Line::from(Span::styled(app.hint(), Style::default().fg(GREEN))).centered(),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_crate(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus_cell() == Focus::Crate;
    let entries = app.filtered();
    let title = format!("Library  ({} items)", entries.len());
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
    // Highlight a row only while a song is actively highlighted (arrows on).
    if app.crate_active && !entries.is_empty() {
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
    // Title = select number + the loaded track, so the grid shows what's where.
    let track = app.pad_source[pad]
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let title = if track.is_empty() {
        format!("{}", pad + 1)
    } else {
        format!("{} {track}", pad + 1)
    };
    draw_cell(
        f,
        area,
        &title,
        vec![Line::from(line1), Line::from(line2)],
        focused,
    );
}

/// The DJ tile's two-line 8-bit cat — bobs while voices play, else rests.
/// A small centered "Quit?" confirmation modal. `y` quits, anything else cancels.
fn draw_quit_modal(f: &mut Frame, area: Rect) {
    draw_confirm_modal(f, area, "Quit?");
}

/// A small centered yes/no modal. `y` confirms, anything else cancels.
fn draw_confirm_modal(f: &mut Frame, area: Rect, prompt: &str) {
    // The question is the title; the body is just the choice.
    let popup = centered((prompt.len() as u16 + 6).max(20), 3, area);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(AMBER))
        .title(format!(" {prompt} "));
    let body = Paragraph::new(Line::from("y / n").centered())
        .block(block)
        .style(Style::default().fg(GREEN));
    f.render_widget(body, popup);
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
        // Enter on the song → Backspace deletes (confirm) → y.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(app.confirm_delete.is_some());
        app.on_key(key('y')); // confirm
        assert!(!tmp.join("gone.wav").exists(), "file deleted");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn move_into_an_existing_folder() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-mv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("box")).unwrap();
        fs::write(tmp.join("t.wav"), b"x").unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        app.set_focus(Focus::Crate);
        // Entries: ["box", "t.wav"] — Down highlights box, Down moves to t.wav.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected_path(), Some(tmp.join("t.wav")));
        // Enter → Right opens the move modal; Down to "box"; Enter moves it.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // (root) → box
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
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
    fn numbers_select_pads_and_timeline_esc_returns_to_library() {
        let mut app = App::new();
        app.set_focus(Focus::Crate);
        assert_eq!(app.on_key(key('3')), Action::Focus);
        assert_eq!(app.focus_cell(), Focus::Pad(2), "3 selects pad 3");
        assert_eq!(app.on_key(key('0')), Action::Focus);
        assert_eq!(app.focus_cell(), Focus::Timeline, "0 selects the timeline");
        app.on_key(key('1'));
        assert_eq!(app.focus_cell(), Focus::Pad(0));
        // Esc from a pad goes up a level → the library.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.focus_cell(), Focus::Crate, "esc returns to the library");
    }

    #[test]
    fn esc_in_a_subfolder_goes_up_not_quit() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-up-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("box")).unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        app.crate_lib.enter(&tmp.join("box")); // now inside box/
        assert_eq!(app.crate_lib.cwd(), tmp.join("box"));
        // Esc in a subfolder goes up to root, does not quit.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.crate_lib.cwd(), tmp.as_path(), "esc went up to root");
        assert!(!app.confirm_quit);
        // Esc at root opens quit.
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.confirm_quit, "esc at root quits");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn move_is_left_in_a_folder_right_at_root() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-mvdir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("box")).unwrap();
        fs::write(tmp.join("box/inside.wav"), b"x").unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        // Inside box/: entries are ["..", "inside.wav"]. Down highlights "..",
        // Down moves to the file.
        app.crate_lib.enter(&tmp.join("box"));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected_path(), Some(tmp.join("box/inside.wav")));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // song prompt
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(
            app.move_picker.is_none(),
            "right is not move inside a folder"
        );
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // prompt again
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(app.move_picker.is_some(), "left moves when inside a folder");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn library_song_actions_work_without_enter() {
        let mut app = App::new();
        app.set_crate(demo_crate());
        app.set_focus(Focus::Crate);
        // Down highlights the current song (no Enter needed).
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(
            app.crate_active && !app.song_action,
            "highlighting, no prompt"
        );
        // Now a number loads directly.
        assert_eq!(app.on_key(key('2')), Action::AssignPad);
        assert_eq!(app.take_pending_pad_load(), Some((1, "/m/a.mp3".into())));
        // r starts a rename directly.
        app.on_key(key('r'));
        assert!(app.rename.is_some(), "r renames directly");
    }

    #[test]
    fn library_numbers_jump_when_nothing_highlighted() {
        let mut app = App::new(); // launches in the library, not highlighting
        app.set_crate(demo_crate());
        assert!(!app.crate_active);
        // With nothing highlighted, a number jumps to that pad.
        assert_eq!(app.on_key(key('3')), Action::Focus);
        assert_eq!(app.focus_cell(), Focus::Pad(2));
    }

    #[test]
    fn library_is_the_default_focus() {
        assert_eq!(App::new().focus_cell(), Focus::Crate);
    }

    #[test]
    fn library_enter_then_number_loads_that_pad() {
        let mut app = App::new();
        app.set_crate(demo_crate());
        app.set_focus(Focus::Crate);
        assert_eq!(app.selected_path(), Some("/m/a.mp3".into()));
        // Enter on a song opens the action prompt (no pad chosen yet).
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Action::Mark
        );
        assert!(app.song_action);
        // Pressing 4 loads it onto pad 4 and closes the prompt.
        assert_eq!(app.on_key(key('4')), Action::AssignPad);
        assert!(!app.song_action);
        assert_eq!(app.take_pending_pad_load(), Some((3, "/m/a.mp3".into())));
    }

    #[test]
    fn move_modal_makes_a_folder_and_moves_a_file_in() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("tk-move2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("song.wav"), b"x").unwrap();
        let mut app = App::new();
        app.set_crate(Crate::scan(&tmp));
        app.set_focus(Focus::Crate);
        // Enter → song prompt → Right opens the move modal.
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.move_picker.is_some());
        // N → type "box" → Enter creates the folder.
        app.on_key(key('n'));
        for c in "box".chars() {
            app.on_key(key(c));
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(tmp.join("box").is_dir(), "folder created");
        // Select the new folder (Down) and Enter to move the file there.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(tmp.join("box/song.wav").exists() && !tmp.join("song.wav").exists());
        assert!(app.move_picker.is_none());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn library_enter_loads_onto_the_selected_pad() {
        let mut app = App::new();
        app.set_crate(demo_crate());
        app.set_focus(Focus::Pad(3)); // remembers pad 3 as the target
        app.set_focus(Focus::Crate); // go to the library
        assert_eq!(app.selected_path(), Some("/m/a.mp3".into()));
        // Menu → load queues the highlighted track onto the last-selected pad.
        assert_eq!(app.run_menu(MenuAction::Load), Action::AssignPad);
        assert_eq!(app.take_pending_pad_load(), Some((3, "/m/a.mp3".into())));
    }

    #[test]
    fn library_browse_selects_entries() {
        let mut app = App::new();
        app.set_crate(demo_crate());
        app.set_focus(Focus::Crate);
        assert_eq!(app.selected_path(), Some("/m/a.mp3".into()));
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // highlight current (a)
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // move to b
        assert_eq!(app.selected_path(), Some("/m/b.mp3".into()));
    }

    #[test]
    fn menu_nudges_global_tempo() {
        let mut app = App::new();
        app.mixer.set_master_bpm(Some(120.0));
        app.set_focus(Focus::Timeline);
        assert_eq!(app.mixer.global_speed(), 1.0);
        app.run_menu(MenuAction::TempoUp);
        assert!(app.mixer.global_speed() > 1.0);
        assert!(app.mixer.effective_bpm().unwrap() > 120.0);
        app.run_menu(MenuAction::TempoDown);
    }

    #[test]
    fn menu_toggles_pad_activation() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.set_focus(Focus::Pad(0));
        assert!(app.mixer.pad_active(0));
        app.run_menu(MenuAction::ToggleActive);
        assert!(!app.mixer.pad_active(0));
    }

    #[test]
    fn clip_edit_marks_audition_and_snips_both_sides() {
        let mut app = App::new();
        app.mixer.set_sample_rate(1000);
        app.mixer.assign_pad(0, vec![0.5; 12_800]); // 6400 frames → step = 6400/64 = 100
        app.set_focus(Focus::Pad(0));
        let enter = || KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.run_menu(MenuAction::EditClip); // menu → edit clip opens the editor
        assert!(app.clip_edit.is_some() && !app.ce_out);
        // Move the IN mark right 5×100 = 500 frames.
        for _ in 0..5 {
            app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        }
        assert_eq!(app.mixer.pad_trim(0).0, 500, "in mark moved");
        // Tab to the OUT mark, pull it left 5×100 → out 5900.
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert!(app.ce_out);
        for _ in 0..5 {
            app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        }
        assert_eq!(app.mixer.pad_trim(0).1, 5900, "out mark moved");
        // Space auditions the selection.
        app.on_key(key(' '));
        assert_eq!(app.mixer.active_voices(), 1, "audition the selection");
        // Enter snips, keeping only [500, 5900) = 5400 frames, reset to [0, len].
        app.on_key(enter());
        assert_eq!(
            app.mixer.pad_clip_frames(0),
            5400,
            "snipped to the selection"
        );
        assert_eq!(
            app.mixer.pad_trim(0),
            (0, 5400),
            "snip becomes the new full clip"
        );
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.clip_edit.is_none());
    }

    #[test]
    fn menu_cut_truncates_the_arrangement() {
        let mut app = App::new();
        app.set_focus(Focus::Timeline);
        app.timeline.set_step(0, 60, true);
        for _ in 0..20 {
            app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)); // cursor → step 20
        }
        app.run_menu(MenuAction::Cut); // cut at step 20 → bar 2 → 32 steps
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
        app.run_menu(MenuAction::Unload);
        assert!(!app.mixer.pad_loaded(0), "pad cleared");
        assert_eq!(app.mixer.pad_kind(0), PadKind::OneShot, "kind reset");
        assert!(app.pad_source[0].is_none());
    }

    #[test]
    fn enter_opens_the_menu_and_runs_an_action() {
        // Enter opens the context menu; arrow to an item; Enter runs it.
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.set_focus(Focus::Pad(0));
        let enter = || KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let down = || KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(app.on_key(enter()), Action::Mark);
        assert!(app.menu.is_some(), "Enter opens the menu");
        // Loaded pad menu: [edit clip, load track here, kind, …] → 3rd is kind.
        app.on_key(down());
        app.on_key(down());
        app.on_key(enter());
        assert!(app.menu.is_none(), "running an item closes the menu");
        assert_eq!(
            app.mixer.pad_kind(0),
            PadKind::Loop,
            "kind cycled via the menu"
        );
    }

    #[test]
    fn arrows_adjust_pad_volume_and_browse_library() {
        let down = || KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up = || KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let mut app = App::new();
        app.set_focus(Focus::Pad(0));
        // On a pad, Up/Down change its volume.
        assert_eq!(app.on_key(down()), Action::Mark);
        assert!(app.mixer.pad_gain(0) < 1.0, "down lowers pad volume");
        app.on_key(up());
        // A number jumps focus (not arrows).
        app.on_key(key('5'));
        assert_eq!(app.focus_cell(), Focus::Pad(4));
        // On the library, Up/Down browse the list.
        app.set_focus(Focus::Crate);
        assert_eq!(app.on_key(down()), Action::CrateNav);
    }

    #[test]
    fn menu_cycles_the_focused_pad_kind() {
        let mut app = App::new();
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.mixer.pad_kind(0), PadKind::OneShot);
        app.run_menu(MenuAction::CycleKind);
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
        assert_eq!(app.run_menu(MenuAction::SaveNew), Action::Record);
        assert!(tmp.join("loop-edit1.wav").exists());
        // Overwrite the source with the (smaller) trimmed clip.
        let before = fs::metadata(&src).unwrap().len();
        assert_eq!(app.run_menu(MenuAction::SaveOver), Action::Record);
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
        assert_eq!(app.run_menu(MenuAction::Render), Action::Record); // render+save
        assert!(
            tmp.join("mix-1.wav").exists(),
            "rendered WAV in the library"
        );
        // It reappears in the listing.
        assert!(app.filtered().iter().any(|(n, _, _)| n == "mix-1.wav"));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn space_plays_the_focused_pad() {
        let mut app = App::new();
        app.set_focus(Focus::Pad(0));
        app.mixer.assign_pad(0, vec![0.5; 64]);
        assert_eq!(app.mixer.active_voices(), 0);
        assert_eq!(app.on_key(key(' ')), Action::TriggerPad);
        assert_eq!(app.mixer.active_voices(), 1, "space plays the focused pad");
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
    }

    #[test]
    fn pad_play_and_timeline_play_are_mutually_exclusive() {
        let mut app = App::new();
        app.mixer.set_master_bpm(Some(120.0));
        app.mixer.assign_pad(0, vec![0.5; 4000]);
        app.mixer.assign_pad(1, vec![0.4; 4000]);
        // Start the timeline.
        app.set_focus(Focus::Timeline);
        app.toggle_transport();
        assert!(app.playing);
        // Triggering a pad stops the timeline (no overlap).
        app.set_focus(Focus::Pad(1));
        app.on_key(key(' '));
        assert!(!app.playing, "playing a pad stops the timeline");
        assert_eq!(app.mixer.active_voices(), 1, "only the live pad sounds");
        // Starting the timeline again clears the live voice.
        app.set_focus(Focus::Timeline);
        app.toggle_transport();
        assert_eq!(app.mixer.active_voices(), 0, "timeline clears live voices");
    }

    #[test]
    fn clip_editor_zoom_makes_arrow_steps_fine() {
        let mut app = App::new();
        app.mixer.set_sample_rate(48_000);
        app.mixer.assign_pad(0, vec![0.5; 48_000 * 2 * 60]); // a long (60s) clip
        app.set_focus(Focus::Pad(0));
        app.run_menu(MenuAction::EditClip);
        assert_eq!(app.ce_zoom, 0, "opens at whole-clip zoom");
        // Whole-clip zoom: one ←/→ moves a big chunk of a long song.
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert!(app.mixer.pad_trim(0).0 > 10_000, "coarse at whole zoom");
        // Zoom all the way in (+ four times → 10ms window).
        app.mixer.set_pad_trim_in(0, 0);
        for _ in 0..4 {
            app.on_key(key('+'));
        }
        assert_eq!(app.ce_zoom, 4);
        // At a 10ms window (480 frames), a step is 480/64 = 7 frames (~0.15ms).
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.mixer.pad_trim(0).0, 7, "fine step when zoomed in");
        // Zoom back out coarsens it again.
        app.on_key(key('-'));
        assert_eq!(app.ce_zoom, 3);
    }

    #[test]
    fn clip_editor_auditions_at_the_active_handle() {
        // The out-handle audition should play the run-up TO out, not from in,
        // so a zoomed-in edit at the end is actually audible.
        let mut m = Mixer::new();
        m.set_sample_rate(1000); // 1.5s preview = 1500 frames
        m.assign_pad(0, vec![0.5; 40_000]); // 20_000 frames
        m.set_pad_trim_out(0, 10_000);
        // Region near the out handle: [10000-1500, 10000) = 1500 frames.
        m.audition_region(0, 10_000usize.saturating_sub(1500), 10_000);
        assert_eq!(m.active_voices(), 1, "auditions a region at the handle");
        // It's a one-shot of ~1500 frames; after 1600 frames it has ended.
        m.fill_mix(&mut vec![0.0; 3200]);
        assert_eq!(
            m.active_voices(),
            0,
            "preview is a short one-shot, not the whole clip"
        );
    }

    #[test]
    fn clip_editor_space_toggles_the_audition() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 4000]);
        app.set_focus(Focus::Pad(0));
        app.run_menu(MenuAction::EditClip); // open editor
        app.on_key(key(' ')); // play the audition
        assert!(app.mixer.pad_is_sounding(0), "space plays");
        app.on_key(key(' ')); // press again → stop
        assert!(!app.mixer.pad_is_sounding(0), "space again pauses");
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
    fn loop_region_fills_across_steps() {
        let mut app = App::new();
        app.set_focus(Focus::Timeline);
        app.toggle_region(); // mark start at step 0, lane 0
        assert!(app.tl_region_start.is_some());
        for _ in 0..4 {
            app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)); // step → 4
        }
        app.toggle_region(); // fill 0..=4 on lane 0
        assert!(app.tl_region_start.is_none());
        for s in 0..=4 {
            assert!(app.timeline.step(0, s), "region filled at {s}");
        }
        assert_eq!(app.timeline.run_at(0, 2), Some((0, 5)));
    }

    #[test]
    fn esc_closes_the_pad_menu() {
        let esc = || KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut app = App::new();
        app.set_focus(Focus::Pad(0)); // pad Enter opens the menu
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.menu.is_some());
        app.on_key(esc());
        assert!(app.menu.is_none(), "esc closes the menu");
        assert!(!app.confirm_quit, "and doesn't quit");
        // A further esc goes up a level to the library.
        app.on_key(esc());
        assert_eq!(app.focus_cell(), Focus::Crate);
    }

    #[test]
    fn esc_leaves_the_clip_editor_then_opens_quit() {
        let esc = || KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.set_focus(Focus::Pad(0));
        app.run_menu(MenuAction::EditClip); // open the clip editor
        assert!(app.clip_edit.is_some());
        app.on_key(esc());
        assert!(app.clip_edit.is_none(), "first esc closes the clip editor");
        assert!(!app.confirm_quit);
        app.on_key(esc()); // pad → library
        assert_eq!(app.focus_cell(), Focus::Crate);
        app.on_key(esc()); // library → quit
        assert!(app.confirm_quit, "esc from the library opens quit");
    }

    #[test]
    fn timeline_place_pad_toggles_a_step_at_the_cursor() {
        let mut app = App::new();
        app.set_focus(Focus::Timeline);
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)); // lane 1
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)); // step 1
        assert_eq!(app.run_menu(MenuAction::PlaceHit), Action::Timeline);
        assert!(app.timeline.step(1, 1), "pad placed at the cursor");
        assert_eq!(app.timeline.pads_at(1), vec![1]);
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
        app.run_menu(MenuAction::ClearPhrase);
        assert_eq!(app.mixer.pad_phrase_len(0), 0);
    }

    #[test]
    fn p_records_a_scratch_phrase_by_tapping() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 8000]);
        app.set_focus(Focus::Pad(0));
        app.mixer.cycle_pad_kind(0);
        app.mixer.cycle_pad_kind(0); // → Scratch
        app.run_menu(MenuAction::PhraseRec); // arm phrase record
        assert_eq!(app.phrase_rec, Some(0));
        let enter = || KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        app.on_key(key(' ')); // tap wiki
        app.on_key(enter()); // tap whip
        app.on_key(key(' ')); // tap wiki
        assert_eq!(app.mixer.pad_phrase_len(0), 3);
        app.run_menu(MenuAction::PhraseRec); // stop recording
        assert_eq!(app.phrase_rec, None);
        // Now space plays the stored phrase as one voice.
        let before = app.mixer.active_voices();
        app.on_key(key(' '));
        assert_eq!(app.mixer.active_voices(), before + 1);
    }

    #[test]
    fn scratch_pad_space_plays_wiki_enter_whips_while_recording() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 4000]);
        app.set_focus(Focus::Pad(0));
        app.mixer.cycle_pad_kind(0); // OneShot → Loop
        app.mixer.cycle_pad_kind(0); // → Scratch
        assert_eq!(app.mixer.pad_kind(0), PadKind::Scratch);
        // Space plays a wiki live.
        assert_eq!(app.on_key(key(' ')), Action::TriggerPad);
        assert_eq!(app.mixer.active_voices(), 1);
        // While recording a phrase, Enter taps a whip (Space taps a wiki).
        app.run_menu(MenuAction::PhraseRec);
        app.on_key(key(' ')); // wiki tap
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)); // whip tap
        assert_eq!(app.mixer.pad_phrase_glyphs(0), "><", "wiki then whip");
    }

    #[test]
    fn pad_trigger_plays_an_assigned_clip() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.on_key(key(' ')), Action::TriggerPad); // space plays
        assert_eq!(app.mixer.active_voices(), 1);
    }

    #[test]
    fn pad_trim_is_nondestructive_via_the_clip_editor() {
        let mut app = App::new();
        app.mixer.set_sample_rate(1000);
        app.mixer.assign_pad(0, vec![0.5; 20_000]); // 10_000 frames
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.mixer.pad_trim(0), (0, 10_000));
        app.run_menu(MenuAction::EditClip); // open editor
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)); // move in-mark
        assert!(app.mixer.pad_trim(0).0 > 0);
        assert_eq!(app.mixer.pad_clip_frames(0), 10_000, "source untouched");
    }

    #[test]
    fn menu_records_the_live_mix_into_the_stash() {
        let mut app = App::new();
        app.set_focus(Focus::Timeline); // not a pad → goes to the stash
        app.mixer.assign_pad(0, vec![0.4; 4096]);
        app.mixer.trigger_pad(0);
        assert_eq!(app.run_menu(MenuAction::Record), Action::Record); // arm
        assert!(app.mixer.is_recording());
        app.mixer.fill_mix(&mut [0.0f32; 256]);
        assert_eq!(app.run_menu(MenuAction::Record), Action::Record); // disarm → stash
        assert!(!app.mixer.is_recording());
        assert_eq!(app.recordings.len(), 1);
    }

    #[test]
    fn menu_adjusts_master_volume() {
        let mut app = App::new();
        app.set_focus(Focus::Timeline);
        let g0 = app.mixer.master_gain();
        app.run_menu(MenuAction::MasterUp);
        assert!(app.mixer.master_gain() > g0);
        app.run_menu(MenuAction::MasterDown);
    }

    #[test]
    fn esc_opens_quit_modal_then_y_quits() {
        let mut app = App::new();
        app.set_focus(Focus::Crate); // esc from the library opens quit
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

        let enter = || KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        // Pad 0 → loop (via menu); pad 1 → clip-edit (trim + snip) then back.
        app.set_focus(Focus::Pad(0));
        app.run_menu(MenuAction::CycleKind);
        assert_eq!(app.mixer.pad_kind(0), PadKind::Loop);
        app.set_focus(Focus::Pad(1));
        app.run_menu(MenuAction::EditClip); // open the clip editor
        for _ in 0..3 {
            app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)); // move in-mark
        }
        app.on_key(key(' ')); // audition
        app.on_key(enter()); // snip
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)); // close
        assert!(app.clip_edit.is_none());

        // Arrange on the timeline: place a hit, play, pause, render.
        app.set_focus(Focus::Timeline);
        app.run_menu(MenuAction::PlaceHit); // place pad 0 at the cursor
        app.on_key(key(' ')); // space plays the arrangement
        app.advance_playback(600);
        app.on_key(key(' ')); // pause
        let mix = app.render_arrangement();
        assert!(
            mix.iter().any(|&s| s.abs() > 0.01),
            "arrangement renders audio"
        );

        // Export pad 1 to mp3 (menu), then unload pad 0 (menu).
        app.set_focus(Focus::Pad(1));
        assert_eq!(app.run_menu(MenuAction::Export), Action::Record);
        assert!(
            fs::read_dir(&tmp).unwrap().any(|e| e
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "mp3")),
            "mp3 exported"
        );
        app.set_focus(Focus::Pad(0));
        app.run_menu(MenuAction::Unload);
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
    fn renders_timeline_wordmark_and_pad_track_titles() {
        let mut app = App::new();
        app.mixer.assign_pad(0, vec![0.5; 64]);
        app.pad_source[0] = Some("/m/break-stuff.mp3".into());
        let text = buffer_text(&render(&app, 96, 40));
        assert!(text.contains("TermKrush"));
        assert!(
            text.contains("TIMELINE"),
            "persistent timeline strip on top"
        );
        // The pad's title now carries its track name (no static "Pad N" label).
        assert!(text.contains("break-stuff"), "pad shows its loaded track");
        assert!(!text.contains("=^.^="), "no DJ cat");
    }
}
