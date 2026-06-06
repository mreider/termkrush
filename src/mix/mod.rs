//! The mixer: owns the decks and combines them onto a master bus.
//!
//! Responsibilities as the backlog fills in: crossfader between decks,
//! per-deck gain, BPM sync and beat-matching, minimal FX (filter, echo,
//! reverb), and the master tap that feeds the recorder.
//!
//! Today it owns the two decks, sums their pull-based output, and applies
//! the **master gain** to the mix. The crossfader and N-deck generality
//! arrive with their own stories; the array makes adding those mechanical.

use std::sync::Arc;

use crate::deck::Deck;

/// Number of decks the mixer drives (two-deck era).
pub const DECKS: usize = 2;

/// Number of sampler pads (clip triggers).
pub const PADS: usize = 7;

/// One playing sampler voice: a shared clip and a position into it.
#[derive(Debug)]
struct SampleVoice {
    clip: Arc<Vec<f32>>, // interleaved stereo at the mix rate
    pos: usize,          // sample index into `clip`
}

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
    /// Per-frame ramp magnitude for the blend. A hard cut sets the blend
    /// directly; an auto-fade sets this so the ramp lands in N seconds.
    xfade_step: f32,
    /// Output sample rate, so timed fades convert seconds to frames.
    sample_rate: u32,
    /// Reusable per-deck scratch for [`fill_mix`](Self::fill_mix), so the
    /// mix path does not allocate every block.
    scratch_a: Vec<f32>,
    scratch_b: Vec<f32>,
    /// Clip assigned to each sampler pad (interleaved stereo), if any.
    pads: [Option<Arc<Vec<f32>>>; PADS],
    /// Manually-set BPM per pad (for later beat-sync / auto-bpm).
    pad_bpm: [Option<f32>; PADS],
    /// Currently-sounding one-shot voices, summed atop the deck mix.
    voices: Vec<SampleVoice>,
    /// When armed, `fill_mix` appends each block of master output here so the
    /// live mix (decks + active pads) can be resampled into a clip.
    recording: bool,
    record_buf: Vec<f32>,
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
            xfade_step: XFADE_RAMP_STEP,
            sample_rate: 44_100,
            scratch_a: Vec::new(),
            scratch_b: Vec::new(),
            pads: Default::default(),
            pad_bpm: Default::default(),
            voices: Vec::new(),
            recording: false,
            record_buf: Vec::new(),
        }
    }

    /// Arm the live-mix recorder: subsequent `fill_mix` output is captured.
    pub fn arm_record(&mut self) {
        self.record_buf.clear();
        self.recording = true;
    }

    /// `true` while the live mix is being captured.
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Disarm and return the captured master output (interleaved stereo).
    pub fn take_recording(&mut self) -> Vec<f32> {
        self.recording = false;
        std::mem::take(&mut self.record_buf)
    }

    /// Assign a decoded clip (interleaved stereo at the mix rate) to pad `i`.
    pub fn assign_pad(&mut self, i: usize, clip: Vec<f32>) {
        if i < PADS {
            self.pads[i] = Some(Arc::new(clip));
        }
    }

    /// Manually nudge pad `i`'s BPM by `delta` (from 120 if unset), clamped.
    pub fn nudge_pad_bpm(&mut self, i: usize, delta: f32) {
        if i < PADS {
            let next = (self.pad_bpm[i].unwrap_or(120.0) + delta).clamp(40.0, 240.0);
            self.pad_bpm[i] = Some(next);
        }
    }

    /// Pad `i`'s manually-set BPM, if any.
    pub fn pad_bpm(&self, i: usize) -> Option<f32> {
        self.pad_bpm.get(i).copied().flatten()
    }

    /// Set pad `i`'s BPM (e.g. carried from a recorded clip on assignment).
    pub fn set_pad_bpm(&mut self, i: usize, bpm: Option<f32>) {
        if i < PADS {
            self.pad_bpm[i] = bpm;
        }
    }

    /// `true` if pad `i` has a clip assigned.
    pub fn pad_loaded(&self, i: usize) -> bool {
        i < PADS && self.pads[i].is_some()
    }

    /// Trigger pad `i`: start a new one-shot voice from the start of its
    /// clip, summed atop whatever the decks are playing. No-op if the pad
    /// is empty. Overlapping triggers stack (polyphonic).
    pub fn trigger_pad(&mut self, i: usize) {
        if let Some(Some(clip)) = self.pads.get(i) {
            self.voices.push(SampleVoice {
                clip: Arc::clone(clip),
                pos: 0,
            });
        }
    }

    /// Number of sampler voices currently sounding.
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// Shared access to deck `i` (panics out of range — callers use `0..DECKS`).
    pub fn deck(&self, i: usize) -> &Deck {
        &self.decks[i]
    }

    /// Mutable access to deck `i`, for transport and loading.
    pub fn deck_mut(&mut self, i: usize) -> &mut Deck {
        &mut self.decks[i]
    }

    /// Set the output sample rate so timed fades know how many frames a
    /// second is. Called once at startup by the event loop.
    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate.max(1);
    }

    /// Instant hard-cut the deck blend to `target` (`-1` = A only, `+1` = B
    /// only). No ramp — for cutting between sources on the beat.
    pub fn cut_to(&mut self, target: f32) {
        self.xfade = target.clamp(XFADE_MIN, XFADE_MAX);
        self.xfade_smoothed = self.xfade;
    }

    /// Begin a hands-free fade to `target` over `secs` seconds. The blend
    /// ramps per frame in `fill_mix`; pace is set so it lands in `secs`.
    pub fn autofade_to(&mut self, target: f32, secs: f32) {
        self.xfade = target.clamp(XFADE_MIN, XFADE_MAX);
        let frames = (secs.max(0.001) * self.sample_rate as f32).max(1.0);
        self.xfade_step = ((self.xfade - self.xfade_smoothed).abs() / frames).max(1e-7);
    }

    /// Current target crossfader position.
    pub fn xfade(&self) -> f32 {
        self.xfade
    }

    /// Currently-applied (ramped) blend — what you actually hear right now.
    pub fn xfade_applied(&self) -> f32 {
        self.xfade_smoothed
    }

    /// True while a fade is still travelling toward its target.
    pub fn is_fading(&self) -> bool {
        (self.xfade - self.xfade_smoothed).abs() > 1e-4
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
                x = (x + self.xfade_step).min(target);
            } else if x > target {
                x = (x - self.xfade_step).max(target);
            }
            let (ga, gb) = xfade_gains(x);
            let l = self.scratch_a[i * 2] * ga + self.scratch_b[i * 2] * gb;
            let r = self.scratch_a[i * 2 + 1] * ga + self.scratch_b[i * 2 + 1] * gb;
            out[i * 2] = l;
            out[i * 2 + 1] = r;
        }
        self.xfade_smoothed = x;

        // Sum the sampler voices on top of the deck mix — they play over
        // whatever the decks are doing, independent of the crossfader.
        // Finished one-shots are dropped.
        self.mix_voices(out);

        self.apply(out);

        // Capture the final master output when armed (post-everything, so a
        // resample includes the decks AND any playing pads — overdub).
        if self.recording {
            self.record_buf.extend_from_slice(out);
        }
    }

    /// Add each active voice's next block into `out` (interleaved stereo),
    /// advancing its position; voices that reach their end are removed.
    fn mix_voices(&mut self, out: &mut [f32]) {
        self.voices.retain_mut(|v| {
            let remaining = v.clip.len().saturating_sub(v.pos);
            let n = out.len().min(remaining);
            for (o, s) in out.iter_mut().zip(v.clip[v.pos..v.pos + n].iter()) {
                *o += *s;
            }
            v.pos += n;
            v.pos < v.clip.len() // keep while not finished
        });
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
    fn cut_clamps_and_is_instant() {
        let mut m = Mixer::new();
        m.cut_to(9.0);
        assert_eq!(m.xfade(), XFADE_MAX);
        assert_eq!(m.xfade_applied(), XFADE_MAX, "hard cut applies immediately");
        assert!(!m.is_fading());
        m.cut_to(-9.0);
        assert_eq!(m.xfade(), XFADE_MIN);
        assert_eq!(m.xfade_applied(), XFADE_MIN);
    }

    #[test]
    fn autofade_ramps_over_the_requested_time() {
        let mut m = Mixer::new();
        m.set_sample_rate(10); // 10 frames per second → easy to count
        m.autofade_to(1.0, 1.0); // travel 0→1 over 10 frames (step 0.1/frame)
        assert!(m.is_fading());
        m.fill_mix(&mut [0.0; 10]); // 5 frames → ~halfway
        assert!((m.xfade_applied() - 0.5).abs() < 0.06 && m.is_fading());
        m.fill_mix(&mut [0.0; 20]); // +10 frames → lands on target
        assert!(!m.is_fading() && (m.xfade_applied() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn full_left_is_deck_a_only_full_right_is_deck_b_only() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(20_000, 0.4)); // A
        m.deck_mut(1).load(track(20_000, 0.6)); // B
        m.deck_mut(0).play();
        m.deck_mut(1).play();

        // Hard cut full left: only A (0.4) is heard, immediately.
        m.cut_to(-1.0);
        let mut buf = vec![0.0f32; 8192];
        m.fill_mix(&mut buf);
        let last = buf[buf.len() - 1];
        assert!(
            (last - 0.4).abs() < 1e-3,
            "full-left should be deck A only, got {last}"
        );

        // Hard cut full right: only B (0.6).
        m.cut_to(1.0);
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
    fn autofade_is_smoothed_no_zipper() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(20_000, 0.4));
        m.deck_mut(1).load(track(20_000, 0.6));
        m.deck_mut(0).play();
        m.deck_mut(1).play();
        // Auto-fade to B over 2s; the first frame must not snap to B-only —
        // it should still be ~both (A+B ≈ 1.0).
        m.autofade_to(1.0, 2.0);
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

    #[test]
    fn pad_assign_trigger_and_one_shot_lifecycle() {
        let mut m = Mixer::new();
        assert!(!m.pad_loaded(0));
        m.assign_pad(0, vec![0.5; 16]); // 8 stereo frames
        assert!(m.pad_loaded(0));

        m.trigger_pad(0);
        assert_eq!(m.active_voices(), 1);

        let mut buf = vec![0.0f32; 8]; // 4 frames — less than the clip
        m.fill_mix(&mut buf);
        assert!(
            buf.iter().all(|&s| (s - 0.5).abs() < 1e-4),
            "clip sums atop silent decks"
        );
        assert_eq!(m.active_voices(), 1, "voice still has tail");

        m.fill_mix(&mut [0.0f32; 8]); // drain the remaining 4 frames
        assert_eq!(m.active_voices(), 0, "one-shot freed at end of clip");
    }

    #[test]
    fn pads_are_polyphonic() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.25; 64]);
        m.trigger_pad(0);
        m.trigger_pad(0);
        assert_eq!(m.active_voices(), 2);
        let mut buf = vec![0.0f32; 8];
        m.fill_mix(&mut buf);
        assert!(
            buf.iter().all(|&s| (s - 0.5).abs() < 1e-4),
            "two voices sum"
        );
    }

    #[test]
    fn live_mix_recorder_captures_master_output() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(20_000, 0.5));
        m.deck_mut(0).play();
        m.assign_pad(0, vec![0.3; 64]);
        m.trigger_pad(0); // a pad voice plays too → overdub into the capture
        m.arm_record();
        assert!(m.is_recording());
        let mut buf = vec![0.0f32; 256];
        m.fill_mix(&mut buf);
        m.fill_mix(&mut buf);
        let rec = m.take_recording();
        assert!(!m.is_recording());
        assert_eq!(rec.len(), 512, "two 256-sample blocks captured");
        assert!(
            rec.iter().any(|&s| s.abs() > 0.01),
            "captured the live deck + pad audio"
        );
    }

    #[test]
    fn triggering_empty_pad_is_a_noop() {
        let mut m = Mixer::new();
        m.trigger_pad(2);
        assert_eq!(m.active_voices(), 0);
    }

    #[test]
    fn pad_plays_over_a_playing_deck() {
        let mut m = Mixer::new();
        m.deck_mut(0).load(track(1000, 0.3));
        m.deck_mut(0).play();
        m.assign_pad(0, vec![0.4; 64]);
        m.trigger_pad(0);
        let mut buf = vec![0.0f32; 8];
        m.fill_mix(&mut buf);
        // deck 0.3 (centered xfade, unity) + pad 0.4 = 0.7 at unity master.
        assert!(
            buf.iter().all(|&s| (s - 0.7).abs() < 1e-4),
            "sample plays over the deck"
        );
    }
}
