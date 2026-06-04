//! Terminal user interface (ratatui + crossterm).
//!
//! Renders the decks, crossfader, library browser, and transport, and
//! routes keyboard input to actions. Keyboard-first: every action is
//! reachable from the keyboard before any mouse consideration. The CRT
//! palette (amber, green, dark background) is honored where reasonable.
//!
//! Placeholder until the TUI-shell story lands.

/// Run the TUI event loop until the user quits.
///
/// Placeholder: the real loop (draw, poll input, dispatch) arrives with
/// the TUI-shell story.
pub fn run() {
    // intentionally empty for the scaffold
}
