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
}

/// Which grid cell currently has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Crate,
    Pad(usize),
    Dj,
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

    // ---- focus order: Crate, Pad0..PADS-1, Dj ------------------------------

    fn focus_order() -> Vec<Focus> {
        let mut v = vec![Focus::Crate];
        v.extend((0..PADS).map(Focus::Pad));
        v.push(Focus::Dj);
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

    fn trigger(&mut self, pad: usize) -> Action {
        self.mixer.trigger_pad(pad);
        Action::TriggerPad
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
            KeyCode::Tab => self.step_focus(1),
            KeyCode::BackTab => self.step_focus(-1),

            // Arrows: on the crate, browse the list; elsewhere move focus.
            KeyCode::Up if self.focus == Focus::Crate => self.crate_nav(-1),
            KeyCode::Down if self.focus == Focus::Crate => self.crate_nav(1),
            KeyCode::Up | KeyCode::Left => self.step_focus(-1),
            KeyCode::Down | KeyCode::Right => self.step_focus(1),

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
            // Library file ops (on the highlighted track).
            KeyCode::Char('x') => self.arm_delete(),
            KeyCode::Char('R') => self.start_rename(),
            KeyCode::Char('m') => self.mark_move(),
            KeyCode::Char('p') => self.paste_move(),
            KeyCode::Char(';') => self.cycle_kind(),
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
            KeyCode::Char('k') => self.assign_recording(),
            KeyCode::Char('a') => self.trim_in(false, shift),
            KeyCode::Char('d') => self.trim_in(true, shift),
            KeyCode::Char('w') => self.trim_out(true, shift),
            KeyCode::Char('s') => self.trim_out(false, shift),
            KeyCode::Char(',') => self.nudge_bpm(false, shift),
            KeyCode::Char('.') => self.nudge_bpm(true, shift),

            // Globals.
            KeyCode::Char('r') => self.toggle_record(),
            KeyCode::Char('[') => {
                self.mixer.nudge_master(-GAIN_NUDGE);
                Action::MasterGain
            }
            KeyCode::Char(']') => {
                self.mixer.nudge_master(GAIN_NUDGE);
                Action::MasterGain
            }
            KeyCode::Char(c @ '1'..='7') => {
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
            self.bpm_cache.insert(d.path, b);
        }
        if i < PADS {
            self.loading[i] = false;
        }
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
    draw_header(f, rows[0]);

    // Body: crate on the left (unless collapsed), pads + DJ on the right.
    let body = if app.crate_collapsed {
        let cols = Layout::horizontal([Constraint::Min(0)]).split(rows[1]);
        draw_pads(f, cols[0], app);
        return_help(f, app);
        return;
    } else {
        Layout::horizontal([Constraint::Length(34), Constraint::Min(0)]).split(rows[1])
    };
    draw_crate(f, body[0], app);
    draw_pads(f, body[1], app);
    return_help(f, app);
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

fn draw_header(f: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            "TermKrush",
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
    // Four rows of two cells: pads 1..7 then the DJ tile.
    let rows = Layout::vertical([Constraint::Ratio(1, 4); 4]).split(area);
    for (row, chunk) in rows.iter().enumerate() {
        let cols = Layout::horizontal([Constraint::Ratio(1, 2); 2]).split(*chunk);
        for (col, cell) in cols.iter().enumerate() {
            let idx = row * 2 + col;
            if idx < PADS {
                draw_pad_cell(f, *cell, app, idx);
            } else {
                draw_cell(f, *cell, "DJ", dj_lines(app), app.focus_cell() == Focus::Dj);
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
    let line1 = if app.is_loading(pad) {
        "  ⏳ loading…".to_string()
    } else {
        format!("  {glyph} {kind}")
    };
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
    draw_cell(
        f,
        area,
        &format!("Pad {}", pad + 1),
        vec![Line::from(line1), Line::from(line2)],
        focused,
    );
}

/// The DJ tile's two-line 8-bit cat — bobs while voices play, else rests.
fn dj_lines(app: &App) -> Vec<Line<'static>> {
    let bobbing = app.mixer.active_voices() > 0 && (app.tick / 8) % 2 == 1;
    let (face, body) = if bobbing {
        ("  =^o^=", "  ♫ DJ ♫")
    } else {
        ("  =^.^=", "  ♫ dj ♫")
    };
    vec![Line::from(face), Line::from(body)]
}

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
  focus   tab / arrows   (library · pads · DJ)
  library / filter   ↑↓ browse   enter open/load→pad   z hide
  files   x delete   R rename   m mark / p move-here
  pad     j play   l load   k assign-rec   ; kind
  trim    a/d in   w/s out   (shift = fine)
  tempo   , / .  pad bpm
  mix     1-7 trigger   r record mix   [ ] master
  quit    esc (y/n)   C-c force   ? help";
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
    fn tab_cycles_focus_through_crate_pads_and_dj() {
        let mut app = App::new();
        app.set_focus(Focus::Crate);
        assert_eq!(app.focus_cell(), Focus::Crate);
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Action::Focus
        );
        assert_eq!(app.focus_cell(), Focus::Pad(0));
        // Step to the end (Dj) and wrap back to Crate.
        for _ in 0..PADS {
            app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        }
        assert_eq!(app.focus_cell(), Focus::Dj);
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
    fn semicolon_cycles_the_focused_pad_kind() {
        let mut app = App::new();
        app.set_focus(Focus::Pad(0));
        assert_eq!(app.mixer.pad_kind(0), PadKind::OneShot);
        assert_eq!(app.on_key(key(';')), Action::Mark);
        assert_eq!(app.mixer.pad_kind(0), PadKind::Loop);
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

    #[test]
    fn renders_wordmark_pads_and_dj() {
        let app = App::new();
        let text = buffer_text(&render(&app, 96, 32));
        assert!(text.contains("TermKrush"));
        assert!(text.contains("Pad 1"));
        assert!(text.contains("=^.^="), "DJ cat renders");
    }
}
