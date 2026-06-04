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
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Paragraph};

/// CRT amber, `#ffb000` — the wordmark and accents.
pub const AMBER: Color = Color::Rgb(0xff, 0xb0, 0x00);
/// CRT green, `#45f07d` — secondary text.
pub const GREEN: Color = Color::Rgb(0x45, 0xf0, 0x7d);
/// Near-black background, `#060907`.
pub const BG: Color = Color::Rgb(0x06, 0x09, 0x07);

/// Redraw cap: poll for input up to this long, giving ~30 Hz when idle.
const FRAME: Duration = Duration::from_millis(33);

/// What an input event asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    ToggleHelp,
}

/// UI state for the shell.
#[derive(Debug, Default)]
pub struct App {
    pub show_help: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        App::default()
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
        match (key.code, key.modifiers) {
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

    // Center a 2-line block (wordmark + tagline) vertically.
    let mid = area.height.saturating_sub(2) / 2;
    let rows = Layout::vertical([
        Constraint::Length(mid),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
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

    let tagline = Paragraph::new("terminal DJ  —  ? help   q quit")
        .alignment(Alignment::Center)
        .style(Style::default().fg(GREEN).bg(BG));
    f.render_widget(tagline, rows[2]);

    if app.show_help {
        draw_help(f, area);
    }
}

/// A centered help overlay (stub: lists the keys it knows so far).
fn draw_help(f: &mut Frame, area: Rect) {
    let w = 40.min(area.width);
    let h = 7.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect::new(x, y, w, h);

    f.render_widget(Clear, popup);
    let help = Paragraph::new("Keys\n\n  ?   toggle this help\n  q   quit\n  C-c quit")
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
    tracing::info!("tui event loop started");
    while !app.should_quit {
        terminal.draw(|f| draw(f, &app))?;
        // Poll up to one frame; redraw at least every FRAME, sooner on input.
        if event::poll(FRAME)? {
            let ev = event::read()?;
            app.on_event(&ev);
        }
    }
    tracing::info!("tui event loop exited");
    Ok(())
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
}
