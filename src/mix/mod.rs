//! The mixer: owns the decks and combines them onto a master bus.
//!
//! Responsibilities as the backlog fills in: crossfader between decks,
//! per-deck gain, BPM sync and beat-matching, minimal FX (filter, echo,
//! reverb), and the master tap that feeds the recorder.
//!
//! Today it owns the two decks, sums their pull-based output, and applies
//! the **master gain** to the mix. The crossfader and N-deck generality
//! arrive with their own stories; the array makes adding those mechanical.

use crate::deck::Deck;

/// Number of decks the mixer drives (two-deck era).
pub const DECKS: usize = 2;

/// Allowed master gain range: silence up to +3.5 dB of headroom.
pub const MASTER_MIN: f32 = 0.0;
pub const MASTER_MAX: f32 = 1.5;

/// Max change in applied master gain per frame, matching the deck's ramp
/// so master moves de-zipper too.
const MASTER_RAMP_STEP: f32 = 1.0 / 512.0;

/// The master bus: owns the decks, sums them, and applies master gain.
#[derive(Debug)]
pub struct Mixer {
    /// The decks, summed into the master mix.
    decks: [Deck; DECKS],
    /// Target master gain (what the dB readout shows).
    master: f32,
    /// Applied master gain, ramped toward `master` per frame.
    smoothed: f32,
    /// Reusable per-deck scratch for [`fill_mix`](Self::fill_mix), so the
    /// mix path does not allocate every block.
    scratch: Vec<f32>,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    /// A master bus at unity gain with empty decks.
    pub fn new() -> Self {
        Mixer {
            decks: [Deck::new(), Deck::new()],
            master: 1.0,
            smoothed: 1.0,
            scratch: Vec::new(),
        }
    }

    /// Shared access to deck `i` (panics out of range — callers use `0..DECKS`).
    pub fn deck(&self, i: usize) -> &Deck {
        &self.decks[i]
    }

    /// Mutable access to deck `i`, for transport and loading.
    pub fn deck_mut(&mut self, i: usize) -> &mut Deck {
        &mut self.decks[i]
    }

    /// Sum every deck's next block into `out` (interleaved stereo) and
    /// apply the master gain. Decks play independently; a stopped/paused
    /// deck contributes silence. This is what the audio pump calls.
    pub fn fill_mix(&mut self, out: &mut [f32]) {
        out.iter_mut().for_each(|s| *s = 0.0);
        self.scratch.resize(out.len(), 0.0);
        for deck in &mut self.decks {
            deck.fill(&mut self.scratch);
            for (o, s) in out.iter_mut().zip(self.scratch.iter()) {
                *o += *s;
            }
        }
        self.apply(out);
    }

    /// Set the target master gain, clamped to `[MASTER_MIN, MASTER_MAX]`.
    pub fn set_master(&mut self, gain: f32) {
        self.master = gain.clamp(MASTER_MIN, MASTER_MAX);
    }

    /// Nudge the master gain by `delta` (clamped). Bound to `<`/`>` in the UI.
    pub fn nudge_master(&mut self, delta: f32) {
        self.set_master(self.master + delta);
    }

    /// Current target master gain (1.0 == unity).
    pub fn master_gain(&self) -> f32 {
        self.master
    }

    /// Apply the master gain to an interleaved-stereo buffer in place,
    /// ramping toward the target one step per frame so changes don't click.
    pub fn apply(&mut self, buf: &mut [f32]) {
        let target = self.master;
        let mut g = self.smoothed;
        for frame in buf.chunks_mut(2) {
            if g < target {
                g = (g + MASTER_RAMP_STEP).min(target);
            } else if g > target {
                g = (g - MASTER_RAMP_STEP).max(target);
            }
            for s in frame.iter_mut() {
                *s *= g;
            }
        }
        self.smoothed = g;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::DecodedAudio;

    /// A constant-level stereo track for summing checks.
    fn track(frames: usize, level: f32) -> DecodedAudio {
        DecodedAudio {
            samples: vec![level; frames * 2],
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
    fn fill_mix_sums_playing_decks() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(1000, 0.2));
        m.deck_mut(1).load(track(1000, 0.3));
        m.deck_mut(0).play();
        m.deck_mut(1).play();
        let mut buf = vec![0.0f32; 64];
        m.fill_mix(&mut buf);
        // 0.2 + 0.3 = 0.5 at unity master.
        assert!(
            buf.iter().all(|&s| (s - 0.5).abs() < 1e-4),
            "decks should sum"
        );
    }

    #[test]
    fn fill_mix_ignores_stopped_deck() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(1000, 0.4));
        m.deck_mut(1).load(track(1000, 0.4));
        m.deck_mut(0).play(); // deck 1 stays loaded (silent)
        let mut buf = vec![0.0f32; 64];
        m.fill_mix(&mut buf);
        assert!(
            buf.iter().all(|&s| (s - 0.4).abs() < 1e-4),
            "only the playing deck contributes"
        );
    }

    #[test]
    fn decks_are_independent() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(1000, 0.5));
        m.deck_mut(1).load(track(1000, 0.5));
        m.deck_mut(0).play();
        // Drain a block; deck 0 advances, deck 1 does not.
        let mut buf = vec![0.0f32; 64];
        m.fill_mix(&mut buf);
        assert!(m.deck(0).position_frames() > 0);
        assert_eq!(
            m.deck(1).position_frames(),
            0,
            "loading/playing one leaves the other put"
        );
    }

    #[test]
    fn master_clamps_to_range() {
        let mut m = Mixer::new();
        m.set_master(99.0);
        assert_eq!(m.master_gain(), MASTER_MAX);
        m.set_master(-1.0);
        assert_eq!(m.master_gain(), MASTER_MIN);
    }

    #[test]
    fn nudge_moves_and_clamps() {
        let mut m = Mixer::new();
        m.nudge_master(0.05);
        assert!((m.master_gain() - 1.05).abs() < 1e-6);
        for _ in 0..100 {
            m.nudge_master(0.05);
        }
        assert_eq!(m.master_gain(), MASTER_MAX, "nudging up clamps at max");
    }

    #[test]
    fn apply_at_unity_is_transparent() {
        let mut m = Mixer::new();
        let mut buf = vec![0.5f32; 16];
        m.apply(&mut buf);
        assert!(buf.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn apply_ramps_without_jumping() {
        let mut m = Mixer::new();
        m.set_master(0.0); // target silence from unity
        let mut buf = vec![1.0f32; 4]; // 2 frames
        m.apply(&mut buf);
        // After one frame the gain has only dropped by a step, not to 0.
        let expected_first = 1.0 - MASTER_RAMP_STEP;
        assert!(
            (buf[0] - expected_first).abs() < 1e-4,
            "first frame should ramp by one step, got {}",
            buf[0]
        );
        assert!(buf[0] > 0.9, "no instantaneous jump to silence");
    }

    #[test]
    fn apply_reaches_target_eventually() {
        let mut m = Mixer::new();
        m.set_master(0.5);
        let mut buf = vec![1.0f32; 4096]; // plenty of frames to converge
        m.apply(&mut buf);
        // The tail is multiplied by the fully-ramped 0.5.
        assert!((buf[buf.len() - 1] - 0.5).abs() < 1e-4);
    }
}
