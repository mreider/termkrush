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

/// Crossfader position range: `-1.0` = deck A only, `+1.0` = deck B only,
/// `0.0` = both decks at unity.
pub const XFADE_MIN: f32 = -1.0;
pub const XFADE_MAX: f32 = 1.0;

/// Max change in applied master gain per frame, matching the deck's ramp
/// so master moves de-zipper too.
const MASTER_RAMP_STEP: f32 = 1.0 / 512.0;

/// Max change in the crossfader position per frame, so slides de-zipper.
const XFADE_RAMP_STEP: f32 = 1.0 / 512.0;

/// Linear crossfader law: position `pos` in `[-1, 1]` to (deck A, deck B)
/// gains. `0` leaves both at unity; the off-side deck ramps to silence as
/// the fader travels to the far end.
fn xfade_gains(pos: f32) -> (f32, f32) {
    (1.0 - pos.max(0.0), 1.0 + pos.min(0.0))
}

/// The master bus: owns the decks, crossfades A↔B, and applies master gain.
#[derive(Debug)]
pub struct Mixer {
    /// The decks: index 0 is deck A, index 1 is deck B.
    decks: [Deck; DECKS],
    /// Target master gain (what the dB readout shows).
    master: f32,
    /// Applied master gain, ramped toward `master` per frame.
    smoothed: f32,
    /// Target crossfader position (`-1` A only … `+1` B only).
    xfade: f32,
    /// Applied crossfader position, ramped toward `xfade` per frame.
    xfade_smoothed: f32,
    /// Reusable per-deck scratch for [`fill_mix`](Self::fill_mix), so the
    /// mix path does not allocate every block.
    scratch_a: Vec<f32>,
    scratch_b: Vec<f32>,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    /// A master bus at unity gain, crossfader centered, empty decks.
    pub fn new() -> Self {
        Mixer {
            decks: [Deck::new(), Deck::new()],
            master: 1.0,
            smoothed: 1.0,
            xfade: 0.0,
            xfade_smoothed: 0.0,
            scratch_a: Vec::new(),
            scratch_b: Vec::new(),
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

    /// Set the crossfader position, clamped to `[-1, 1]`.
    pub fn set_xfade(&mut self, pos: f32) {
        self.xfade = pos.clamp(XFADE_MIN, XFADE_MAX);
    }

    /// Nudge the crossfader by `delta` (clamped). Bound to `[`/`]` in the UI.
    pub fn nudge_xfade(&mut self, delta: f32) {
        self.set_xfade(self.xfade + delta);
    }

    /// Return the crossfader to center (both decks at unity). Bound to `\`.
    pub fn center_xfade(&mut self) {
        self.xfade = 0.0;
    }

    /// Current target crossfader position.
    pub fn xfade(&self) -> f32 {
        self.xfade
    }

    /// Mix the two decks through the crossfader into `out` (interleaved
    /// stereo) and apply the master gain. Decks play independently; a
    /// stopped/paused deck contributes silence. The crossfader position is
    /// ramped per frame so slides don't click. This is what the pump calls.
    pub fn fill_mix(&mut self, out: &mut [f32]) {
        self.scratch_a.resize(out.len(), 0.0);
        self.scratch_b.resize(out.len(), 0.0);
        self.decks[0].fill(&mut self.scratch_a);
        self.decks[1].fill(&mut self.scratch_b);

        let target = self.xfade;
        let mut x = self.xfade_smoothed;
        for i in 0..out.len() / 2 {
            // Ramp the fader toward its target, then derive the A/B gains.
            if x < target {
                x = (x + XFADE_RAMP_STEP).min(target);
            } else if x > target {
                x = (x - XFADE_RAMP_STEP).max(target);
            }
            let (ga, gb) = xfade_gains(x);
            let l = self.scratch_a[i * 2] * ga + self.scratch_b[i * 2] * gb;
            let r = self.scratch_a[i * 2 + 1] * ga + self.scratch_b[i * 2 + 1] * gb;
            out[i * 2] = l;
            out[i * 2 + 1] = r;
        }
        self.xfade_smoothed = x;

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
    fn xfade_gains_follow_linear_law() {
        assert_eq!(xfade_gains(0.0), (1.0, 1.0), "center: both unity");
        assert_eq!(xfade_gains(1.0), (0.0, 1.0), "+1: B only");
        assert_eq!(xfade_gains(-1.0), (1.0, 0.0), "-1: A only");
        let (a, b) = xfade_gains(0.5);
        assert!((a - 0.5).abs() < 1e-6 && (b - 1.0).abs() < 1e-6);
        let (a, b) = xfade_gains(-0.5);
        assert!((a - 1.0).abs() < 1e-6 && (b - 0.5).abs() < 1e-6);
    }

    #[test]
    fn xfade_clamps_and_centers() {
        let mut m = Mixer::new();
        m.set_xfade(9.0);
        assert_eq!(m.xfade(), XFADE_MAX);
        m.set_xfade(-9.0);
        assert_eq!(m.xfade(), XFADE_MIN);
        m.nudge_xfade(0.5);
        assert!((m.xfade() - (-0.5)).abs() < 1e-6);
        m.center_xfade();
        assert_eq!(m.xfade(), 0.0);
    }

    #[test]
    fn full_left_is_deck_a_only_full_right_is_deck_b_only() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(20_000, 0.4)); // A
        m.deck_mut(1).load(track(20_000, 0.6)); // B
        m.deck_mut(0).play();
        m.deck_mut(1).play();

        // Full left: after the fader ramps, only A (0.4) is heard.
        m.set_xfade(-1.0);
        let mut buf = vec![0.0f32; 8192];
        m.fill_mix(&mut buf);
        let last = buf[buf.len() - 1];
        assert!(
            (last - 0.4).abs() < 1e-3,
            "full-left should be deck A only, got {last}"
        );

        // Full right: only B (0.6).
        m.set_xfade(1.0);
        let mut buf = vec![0.0f32; 8192];
        m.fill_mix(&mut buf);
        let last = buf[buf.len() - 1];
        assert!(
            (last - 0.6).abs() < 1e-3,
            "full-right should be deck B only, got {last}"
        );
    }

    #[test]
    fn center_plays_both_at_unity() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(1000, 0.3));
        m.deck_mut(1).load(track(1000, 0.3));
        m.deck_mut(0).play();
        m.deck_mut(1).play();
        // Default fader is centered.
        let mut buf = vec![0.0f32; 64];
        m.fill_mix(&mut buf);
        assert!(
            buf.iter().all(|&s| (s - 0.6).abs() < 1e-4),
            "center: A+B at unity"
        );
    }

    #[test]
    fn xfade_slide_is_smoothed() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(20_000, 0.4));
        m.deck_mut(1).load(track(20_000, 0.6));
        m.deck_mut(0).play();
        m.deck_mut(1).play();
        // Jump the target hard to +1; the first frame must not snap to
        // B-only — it should still be ~both (A+B ≈ 1.0).
        m.set_xfade(1.0);
        let mut buf = vec![0.0f32; 8];
        m.fill_mix(&mut buf);
        assert!(
            (buf[0] - 1.0).abs() < 0.02,
            "first frame should barely move from center (no zipper), got {}",
            buf[0]
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
