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

use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph};

use crate::audio::AudioOutput;
use crate::config::Config;
use crate::deck::{Deck, DeckState};
use crate::library::Crate;
use crate::mix::{Mixer, DECKS};

/// Display labels for the decks, indexed by deck number.
const DECK_LABELS: [&str; DECKS] = ["A", "B"];

/// Per-keypress gain nudge (linear), for both deck and master.
const GAIN_NUDGE: f32 = 0.05;

/// Per-keypress crossfader slide.
const XFADE_NUDGE: f32 = 0.05;

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
}

/// UI state for the shell: the decks + master bus (owned by [`Mixer`]),
/// which deck has focus, and the browsable local crate.
#[derive(Debug, Default)]
pub struct App {
    pub show_help: bool,
    pub should_quit: bool,
    pub mixer: Mixer,
    /// Which deck the transport keys target (`Tab` cycles).
    focus: usize,
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
}

impl App {
    pub fn new() -> Self {
        App::default()
    }

    /// Install a freshly-scanned crate, resetting the selection.
    pub fn set_crate(&mut self, crate_lib: Crate) {
        self.crate_lib = crate_lib;
        self.crate_sel = 0;
    }

    /// The focused deck (transport target), shared.
    fn focused(&self) -> &Deck {
        self.mixer.deck(self.focus)
    }

    /// The focused deck, mutable.
    fn focused_mut(&mut self) -> &mut Deck {
        self.mixer.deck_mut(self.focus)
    }

    /// Which deck currently has focus (`0..DECKS`).
    pub fn focus(&self) -> usize {
        self.focus
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
        let seek_amt = if shift { SEEK_FAR } else { SEEK_JUMP };
        match (key.code, key.modifiers) {
            // ---- global / out of the play cluster ----
            (KeyCode::Char('q'), _) => {
                self.should_quit = true;
                Action::Quit
            }
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
                Action::Quit
            }
            (KeyCode::Char('?'), _) => {
                self.show_help = !self.show_help;
                Action::ToggleHelp
            }
            (KeyCode::Esc, _) if self.show_help => {
                self.show_help = false;
                Action::ToggleHelp
            }

            // ---- DECK A — left hand ----
            // index home = play, middle home = cue, ring column = volume,
            // index/middle top = seek.
            (KeyCode::Char('f'), _) => {
                self.mixer.deck_mut(0).toggle();
                Action::PlayPause
            }
            (KeyCode::Char('d'), _) => {
                self.mixer.deck_mut(0).stop();
                Action::Stop
            }
            (KeyCode::Char('w'), _) => {
                self.mixer.deck_mut(0).nudge_gain(GAIN_NUDGE);
                Action::DeckGain
            }
            (KeyCode::Char('s'), _) => {
                self.mixer.deck_mut(0).nudge_gain(-GAIN_NUDGE);
                Action::DeckGain
            }
            (KeyCode::Char('e'), _) => {
                self.mixer.deck_mut(0).seek_by(-seek_amt);
                Action::Seek
            }
            (KeyCode::Char('r'), _) => {
                self.mixer.deck_mut(0).seek_by(seek_amt);
                Action::Seek
            }

            // ---- DECK B — right hand (mirror) ----
            (KeyCode::Char('j'), _) => {
                self.mixer.deck_mut(1).toggle();
                Action::PlayPause
            }
            (KeyCode::Char('k'), _) => {
                self.mixer.deck_mut(1).stop();
                Action::Stop
            }
            (KeyCode::Char('o'), _) => {
                self.mixer.deck_mut(1).nudge_gain(GAIN_NUDGE);
                Action::DeckGain
            }
            (KeyCode::Char('l'), _) => {
                self.mixer.deck_mut(1).nudge_gain(-GAIN_NUDGE);
                Action::DeckGain
            }
            (KeyCode::Char('i'), _) => {
                self.mixer.deck_mut(1).seek_by(-seek_amt);
                Action::Seek
            }
            (KeyCode::Char('u'), _) => {
                self.mixer.deck_mut(1).seek_by(seek_amt);
                Action::Seek
            }

            // ---- crossfader — between the hands (index inner reach) ----
            (KeyCode::Char('g'), _) => {
                self.mixer.nudge_xfade(-XFADE_NUDGE); // toward deck A
                Action::Crossfade
            }
            (KeyCode::Char('h'), _) => {
                self.mixer.nudge_xfade(XFADE_NUDGE); // toward deck B
                Action::Crossfade
            }
            (KeyCode::Char(' '), _) => {
                self.mixer.center_xfade(); // big neutral key = recenter
                Action::Crossfade
            }

            // ---- master + fine scrub (focused deck) ----
            (KeyCode::Char('['), _) => {
                self.mixer.nudge_master(-GAIN_NUDGE);
                Action::MasterGain
            }
            (KeyCode::Char(']'), _) => {
                self.mixer.nudge_master(GAIN_NUDGE);
                Action::MasterGain
            }
            (KeyCode::Char(','), _) => {
                self.focused_mut().seek_by(-SEEK_SCRUB);
                Action::Seek
            }
            (KeyCode::Char('.'), _) => {
                self.focused_mut().seek_by(SEEK_SCRUB);
                Action::Seek
            }

            // ---- crate browser + deck focus (for load / fine scrub target) ----
            (KeyCode::Tab, _) => {
                self.focus = (self.focus + 1) % DECKS;
                Action::Focus
            }
            (KeyCode::Char('/'), _) => {
                self.filter = Some(String::new());
                self.crate_sel = 0;
                Action::Filter
            }
            (KeyCode::Up, _) => {
                self.sel_up();
                Action::CrateNav
            }
            (KeyCode::Down, _) => {
                self.sel_down();
                Action::CrateNav
            }
            (KeyCode::Enter, _) => {
                self.pending_load = self.selected_path();
                if self.pending_load.is_some() {
                    Action::LoadSelected
                } else {
                    Action::None
                }
            }
            (KeyCode::Char('z'), _) => self.crate_collapse_toggle(),
            (KeyCode::Char('\\'), _) => Action::OpenFile, // load demo into focused deck

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
    let tagline = Paragraph::new("A f·play d·cue   g/h xfade   B j·play k·cue    ? help   q quit")
        .alignment(Alignment::Center)
        .style(Style::default().fg(GREEN).bg(BG));
    f.render_widget(tagline, rows[2]);

    // Body: an optional crate browser on the left, the mixer area on the
    // right. The crate collapses (key `c`) to give the decks full width.
    let mixer_area = if app.crate_collapsed {
        rows[3]
    } else {
        let body = Layout::horizontal([Constraint::Length(32), Constraint::Min(0)]).split(rows[3]);
        draw_crate_panel(f, body[0], app);
        body[1]
    };

    // Mixer area: decks A | B side by side on top, the mixer row beneath.
    let stack = Layout::vertical([
        Constraint::Length(8), // decks row
        Constraint::Length(4), // mixer row (crossfader + master)
        Constraint::Min(0),
    ])
    .split(mixer_area);

    let deck_cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(stack[0]);
    for i in 0..DECKS {
        draw_deck_panel(
            f,
            deck_cols[i],
            DECK_LABELS[i],
            app.mixer.deck(i),
            app.focus() == i,
        );
    }

    draw_mixer_panel(f, stack[1], app);

    if app.show_help {
        draw_help(f, area);
    }
}

/// The mixer row: a bordered panel with the crossfader fader graphic over
/// the master readout, sitting beneath the two decks.
fn draw_mixer_panel(f: &mut Frame, area: Rect, app: &App) {
    let lines = vec![
        Line::from(format!("A {} B", crossfader_bar(app.mixer.xfade(), 21)))
            .style(Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
        Line::from(format!(
            "master {:.2}  {}",
            app.mixer.master_gain(),
            fmt_db(app.mixer.master_gain()),
        )),
    ];
    let panel = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .block(
            Block::bordered()
                .title("Mixer")
                .style(Style::default().fg(GREEN).bg(BG)),
        )
        .style(Style::default().fg(GREEN).bg(BG));
    f.render_widget(panel, area);
}

/// Render the crate browser: a bordered, scrollable list of tracks with
/// the highlight at the current selection. The block title shows the
/// track count, or the live filter query while `/` is open.
fn draw_crate_panel(f: &mut Frame, area: Rect, app: &App) {
    let visible = app.visible();
    let title = match &app.filter {
        Some(q) => format!("Crate  /{q}_  ({} match)", visible.len()),
        None => format!("Crate  ({} tracks)", app.crate_lib.len()),
    };

    let items: Vec<ListItem> = if visible.is_empty() {
        vec![ListItem::new(if app.crate_lib.is_empty() {
            "  (no mp3s — set crate_root, see README)"
        } else {
            "  (no matches)"
        })]
    } else {
        visible
            .iter()
            .map(|e| ListItem::new(e.name.clone()))
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(title)
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

/// A deck panel: track name, transport glyph + state + gain, a
/// proportional position bar, and `elapsed / total`. The focused deck has
/// an amber border and a `▸` marker; unfocused decks are dim.
fn draw_deck_panel(f: &mut Frame, area: Rect, label: &str, deck: &Deck, focused: bool) {
    let border = deck_border(focused);
    let marker = if focused { "▸ " } else { "  " };
    // Show the detected tempo in the title once analysis completes.
    let bpm = match deck.bpm() {
        Some(b) => format!("  {b:.0} BPM"),
        None => String::new(),
    };
    let title = format!("{marker}Deck {label}{bpm}");

    let name = deck.display_name().unwrap_or("— no track —");
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

    // Bar width = inner width minus the two brackets, capped for tidiness.
    let bar_w = (area.width.saturating_sub(2) as usize)
        .saturating_sub(2)
        .min(48);

    let lines = vec![
        Line::from(Span::styled(
            name.to_string(),
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            "{} {state_word}    gain {:.2}  {}",
            transport_glyph(deck.state()),
            deck.gain(),
            fmt_db(deck.gain()),
        )),
        Line::from(progress_bar(frac, bar_w)),
        Line::from(format!("{} / {}", fmt_clock(elapsed), fmt_clock(total))),
    ];

    let panel = Paragraph::new(lines)
        .block(
            Block::bordered()
                .title(title)
                .style(Style::default().fg(border).bg(BG)),
        )
        .style(Style::default().fg(GREEN).bg(BG));
    f.render_widget(panel, area);
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

/// A `|───●───|` crossfader slider `width` cells wide between the bars,
/// with the handle `●` at `pos` in `[-1, 1]` (`-1` left, `0` center, `+1`
/// right). `width` should be odd so center lands on a cell.
fn crossfader_bar(pos: f32, width: usize) -> String {
    let pos = pos.clamp(-1.0, 1.0);
    let last = width.saturating_sub(1);
    // Map [-1, 1] -> [0, width-1].
    let handle = (((pos + 1.0) / 2.0) * last as f32).round() as usize;
    let mut s = String::with_capacity(width + 2);
    s.push('|');
    for i in 0..width {
        s.push(if i == handle { '●' } else { '─' });
    }
    s.push('|');
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
    let w = 56.min(area.width);
    let h = 22.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);
    // Keys are by finger position: left hand = deck A, right hand = deck B.
    let help = Paragraph::new(
        "Keys  —  left hand = A,  right hand = B\n\n              DECK A        DECK B\n  play/pause    f             j\n  cue (stop)    d             k\n  volume +/-    w / s         o / l\n  seek -/+      e / r         i / u    (shift: far)\n\n  crossfader    g  ◄A   B►  h     space  center\n  master  -/+   [ / ]\n  fine scrub    , / .   (focused deck)\n\n  crate   tab focus   / filter   ↑/↓ pick   enter load\n          \\ load demo   z hide crate\n  ?  help     q / C-c  quit",
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
    let mut scratch: Vec<f32> = Vec::new();

    // Background BPM detection posts (deck index, bpm) back to the UI loop.
    let (bpm_tx, bpm_rx) = std::sync::mpsc::channel::<(usize, f32)>();

    tracing::info!("tui event loop started");
    while !app.should_quit {
        terminal.draw(|f| draw(f, &app))?;
        // Poll up to one frame; redraw at least every FRAME, sooner on input.
        if event::poll(FRAME)? {
            let ev = event::read()?;
            let focus = app.focus();
            match app.on_event(&ev) {
                Action::OpenFile => {
                    let path = demo_track_path();
                    load_into(&mut app.mixer, focus, &path, target_rate, &bpm_tx);
                }
                Action::LoadSelected => {
                    if let Some(path) = app.take_pending_load() {
                        load_into(&mut app.mixer, focus, &path, target_rate, &bpm_tx);
                    }
                }
                _ => {}
            }
        }
        // Apply any completed background BPM detections.
        while let Ok((idx, bpm)) = bpm_rx.try_recv() {
            app.mixer.deck_mut(idx).set_bpm(bpm);
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

/// Decode `path` at the output rate, load it into deck `idx`, and kick off
/// background BPM detection that posts its result back via `bpm_tx`. A
/// decode failure is logged, not fatal, so the UI keeps running.
fn load_into(
    mixer: &mut Mixer,
    idx: usize,
    path: &Path,
    target_rate: u32,
    bpm_tx: &std::sync::mpsc::Sender<(usize, f32)>,
) {
    match crate::audio::decode_file(path, target_rate) {
        Ok(track) => {
            tracing::info!(path = %path.display(), frames = track.frames(), "deck: loaded track");

            // Detect tempo off the UI thread on a clone of the samples, so a
            // ~0.5s analysis never blocks playback or redraw.
            let (samples, ch, sr) = (track.samples.clone(), track.channels, track.sample_rate);
            let tx = bpm_tx.clone();
            std::thread::spawn(move || {
                if let Some(bpm) = crate::audio::detect_bpm(&samples, ch, sr) {
                    tracing::info!(deck = idx, bpm, "bpm: detected");
                    let _ = tx.send((idx, bpm));
                } else {
                    tracing::info!(deck = idx, "bpm: no tempo detected");
                }
            });

            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("track")
                .to_string();
            mixer.deck_mut(idx).load_named(track, name);
        }
        Err(e) => tracing::error!(error = %e, path = %path.display(), "deck: failed to load track"),
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
        assert!(text.contains("q quit"), "tagline missing:\n{text}");
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

    #[test]
    fn q_quits() {
        let mut app = App::new();
        assert_eq!(app.on_key(key('q')), Action::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new();
        let ev = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(app.on_key(ev), Action::Quit);
        assert!(app.should_quit);
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
    fn f_toggles_deck_a_play_pause() {
        let mut app = loaded_app(); // track on deck A (focus 0)
        assert_eq!(app.mixer.deck(0).state(), DeckState::Loaded);
        assert_eq!(app.on_key(key('f')), Action::PlayPause);
        assert_eq!(app.mixer.deck(0).state(), DeckState::Playing);
        assert_eq!(app.on_key(key('f')), Action::PlayPause);
        assert_eq!(app.mixer.deck(0).state(), DeckState::Paused);
    }

    #[test]
    fn d_stops_deck_a() {
        let mut app = loaded_app();
        app.on_key(key('f')); // play A
        assert_eq!(app.on_key(key('d')), Action::Stop);
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
        let app = app_with_track(1000, 100); // 10.0s
        let text = buffer_text(&render(&app, 80, 24));
        assert!(text.contains("sine_a440_10s.wav"), "title missing:\n{text}");
        assert!(text.contains("00:10.0"), "total time missing:\n{text}");
    }

    #[test]
    fn panel_elapsed_advances_then_freezes_and_glyph_changes() {
        let mut app = app_with_track(1000, 100); // 10.0s, rate 100 on deck A
        app.on_key(key('f')); // play A

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
        app.on_key(key('f'));
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
    fn w_s_nudge_deck_a_gain_o_l_nudge_deck_b() {
        let mut app = loaded_app(); // track on A
        assert_eq!(app.on_key(key('w')), Action::DeckGain); // A up
        assert!((app.mixer.deck(0).gain() - 1.05).abs() < 1e-6);
        assert_eq!(app.on_key(key('s')), Action::DeckGain); // A down
        assert!((app.mixer.deck(0).gain() - 1.0).abs() < 1e-6);
        // Right-hand keys drive deck B's gain, not deck A's.
        assert_eq!(app.on_key(key('o')), Action::DeckGain); // B up
        assert!((app.mixer.deck(1).gain() - 1.05).abs() < 1e-6);
        assert_eq!(app.on_key(key('l')), Action::DeckGain); // B down
        assert!((app.mixer.deck(1).gain() - 1.0).abs() < 1e-6);
        assert!(
            (app.mixer.deck(0).gain() - 1.0).abs() < 1e-6,
            "deck A untouched by B keys"
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
        let text = buffer_text(&render(&app, 80, 24));
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
    fn e_r_seek_deck_a() {
        let mut app = app_with_track(2000, 100); // 20s on deck A
        assert_eq!(app.on_key(key('r')), Action::Seek); // forward
        assert!(
            (app.mixer.deck(0).position_secs() - 5.0).abs() < 1e-9,
            "r => +5s"
        );
        app.on_key(key('e')); // back
        assert!(
            (app.mixer.deck(0).position_secs() - 0.0).abs() < 1e-9,
            "e => -5s"
        );
        app.on_key(key('e'));
        assert_eq!(app.mixer.deck(0).position_frames(), 0, "clamps at start");
    }

    #[test]
    fn shift_seek_is_far_and_eof_stops() {
        let mut app = app_with_track(2000, 100); // 20s on deck A
                                                 // Shift+r = +30s, past the 20s end => clamp to EOF and stop.
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::SHIFT)),
            Action::Seek
        );
        assert_eq!(app.mixer.deck(0).position_frames(), 2000, "clamped to EOF");
        assert_eq!(app.mixer.deck(0).state(), DeckState::Stopped);
    }

    #[test]
    fn comma_period_scrub_focused_deck_finely() {
        let mut app = app_with_track(2000, 100); // deck A focused
        app.on_key(key('r')); // 5.0s
        assert_eq!(app.on_key(key('.')), Action::Seek);
        assert!((app.focused_mut().position_secs() - 5.1).abs() < 1e-9);
        assert_eq!(app.on_key(key(',')), Action::Seek);
        assert!((app.focused_mut().position_secs() - 5.0).abs() < 1e-9);
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
    fn arrows_navigate_and_enter_loads_selected() {
        let mut app = app_with_crate(&["alpha.mp3", "beta.mp3", "gamma.mp3"]);
        let down = || KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up = || KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
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
    fn arrow_navigation_clamps_at_ends() {
        let mut app = app_with_crate(&["a.mp3", "b.mp3"]);
        let down = || KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let up = || KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
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
    fn tab_cycles_focus() {
        let mut app = App::new();
        assert_eq!(app.focus(), 0);
        assert_eq!(
            app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Action::Focus
        );
        assert_eq!(app.focus(), 1);
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus(), 0, "wraps back around");
    }

    #[test]
    fn each_hand_drives_its_own_deck_independently() {
        let mut app = App::new();
        app.mixer.deck_mut(0).load(synth_track(2000));
        app.mixer.deck_mut(1).load(synth_track(2000));

        // Left hand: `f` plays deck A only.
        app.on_key(key('f'));
        assert_eq!(app.mixer.deck(0).state(), DeckState::Playing);
        assert_eq!(app.mixer.deck(1).state(), DeckState::Loaded);

        // Right hand: `j` plays deck B — both now play simultaneously.
        app.on_key(key('j'));
        assert_eq!(
            app.mixer.deck(0).state(),
            DeckState::Playing,
            "A keeps playing"
        );
        assert_eq!(app.mixer.deck(1).state(), DeckState::Playing);

        // `k` stops only deck B; deck A is undisturbed (no focus involved).
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
        // Crossfader fader graphic in the mixer row.
        assert!(text.contains('●'), "crossfader graphic missing:\n{text}");
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
    fn deck_border_is_amber_focused_dim_unfocused() {
        // Acceptance: focus border colors match the design.
        assert_eq!(deck_border(true), AMBER);
        assert_eq!(deck_border(false), Color::DarkGray);
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
    fn gh_slide_and_space_centers_crossfader() {
        let mut app = App::new();
        assert_eq!(app.on_key(key('g')), Action::Crossfade);
        assert!(
            (app.mixer.xfade() - (-0.05)).abs() < 1e-6,
            "g slides toward A"
        );
        assert_eq!(app.on_key(key('h')), Action::Crossfade);
        assert!(app.mixer.xfade().abs() < 1e-6, "h slides back toward B");
        app.on_key(key('h')); // now +0.05
        assert_eq!(app.on_key(key(' ')), Action::Crossfade);
        assert_eq!(app.mixer.xfade(), 0.0, "space re-centers");
    }

    #[test]
    fn crossfader_bar_places_handle() {
        // 21-wide bar: handle index 0 (left), 10 (center), 20 (right).
        assert!(crossfader_bar(-1.0, 21).starts_with("|●"));
        assert!(crossfader_bar(1.0, 21).ends_with("●|"));
        let centered = crossfader_bar(0.0, 21);
        // '|' at byte 0, then cells; center handle is the 11th char.
        assert_eq!(centered.chars().nth(11), Some('●'));
    }

    #[test]
    fn crossfader_renders_in_panel() {
        let app = App::new();
        let text = buffer_text(&render(&app, 100, 28));
        assert!(text.contains('●'), "crossfader handle missing:\n{text}");
    }
}
