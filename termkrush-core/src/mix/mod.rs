//! The mixer: the master bus over the sampler pads.
//!
//! It owns the sampler pads, sums their currently-sounding one-shot voices,
//! applies the **master gain**, and feeds the live-mix recorder. Pad types
//! (loop / scratch), tempo sync, and the arrangement render layer on top in
//! their own stories.

use std::sync::Arc;

/// Number of sampler pads (clip triggers).
pub const PADS: usize = 7;

/// What kind of pad this is — determines its controls and playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PadKind {
    /// Plays its clip once on trigger.
    #[default]
    OneShot,
    /// Repeats, auto-synced to the master tempo.
    Loop,
    /// A short clip scratched with whip/wiki phrases.
    Scratch,
}

/// One playing sampler voice — plays its trimmed clip region forward once.
/// (Loop and scratch behaviours arrive with their own pad-type stories.)
#[derive(Debug)]
struct SampleVoice {
    clip: Arc<Vec<f32>>, // interleaved stereo at the mix rate
    pad: usize,          // owning pad, for live per-pad volume
    in_f: usize,         // trim in-point (frames)
    len_f: usize,        // playable length in frames
    t: usize,            // frames elapsed
}

impl SampleVoice {
    /// The next output frame, or `None` once the voice is finished.
    fn next_frame(&mut self) -> Option<(f32, f32)> {
        if self.t >= self.len_f {
            return None;
        }
        let total = self.clip.len() / 2;
        let idx = (self.in_f + self.t).min(total.saturating_sub(1));
        self.t += 1;
        Some((self.clip[idx * 2], self.clip[idx * 2 + 1]))
    }

    fn done(&self) -> bool {
        self.t >= self.len_f
    }
}

/// Allowed master gain range: silence up to +3.5 dB of headroom.
pub const MASTER_MIN: f32 = 0.0;
pub const MASTER_MAX: f32 = 1.5;

/// Max change in applied master gain per frame, so master moves de-zipper.
const MASTER_RAMP_STEP: f32 = 1.0 / 512.0;

/// The master bus: owns the sampler pads/voices and applies the master gain.
#[derive(Debug)]
pub struct Mixer {
    /// Target master gain (what the dB readout shows).
    master: f32,
    /// Applied master gain, ramped toward `master` per frame.
    smoothed: f32,
    /// Output sample rate, for future tempo-synced features (seconds→frames).
    sample_rate: u32,
    /// Clip assigned to each sampler pad (interleaved stereo), if any.
    pads: [Option<Arc<Vec<f32>>>; PADS],
    /// Manually-set BPM per pad (for loop sync, set on load).
    pad_bpm: [Option<f32>; PADS],
    /// Non-destructive trim bounds per pad, in frames `(in, out)`. The clip
    /// samples are never modified; triggering plays only `[in, out)`.
    pad_trim: [(usize, usize); PADS],
    /// Per-pad kind (one-shot / loop / scratch).
    pad_kind: [PadKind; PADS],
    /// Per-pad linear volume (1.0 = unity), applied live to its voices.
    pad_gain: [f32; PADS],
    /// Currently-sounding one-shot voices, summed onto the master bus.
    voices: Vec<SampleVoice>,
    /// When armed, `fill_mix` appends each block of master output here so the
    /// live mix (active pads) can be resampled into a clip.
    recording: bool,
    record_buf: Vec<f32>,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    /// A master bus at unity gain with empty pads.
    pub fn new() -> Self {
        Mixer {
            master: 1.0,
            smoothed: 1.0,
            sample_rate: 44_100,
            pads: Default::default(),
            pad_bpm: Default::default(),
            pad_trim: Default::default(),
            pad_kind: Default::default(),
            pad_gain: [1.0; PADS],
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
            let frames = clip.len() / 2;
            self.pads[i] = Some(Arc::new(clip));
            self.pad_trim[i] = (0, frames); // full clip by default
        }
    }

    /// Pad `i`'s trim bounds in frames `(in, out)`.
    pub fn pad_trim(&self, i: usize) -> (usize, usize) {
        self.pad_trim.get(i).copied().unwrap_or((0, 0))
    }

    /// Clip length (frames) on pad `i`, 0 if empty.
    pub fn pad_clip_frames(&self, i: usize) -> usize {
        self.pads
            .get(i)
            .and_then(|p| p.as_ref())
            .map(|c| c.len() / 2)
            .unwrap_or(0)
    }

    /// Non-destructively move pad `i`'s in-point by `delta` frames, clamped
    /// to `[0, out-1]`. The samples are untouched — only playback bounds.
    pub fn nudge_pad_in(&mut self, i: usize, delta: i64) {
        if i >= PADS {
            return;
        }
        let (inp, out) = self.pad_trim[i];
        let inp = (inp as i64 + delta).clamp(0, out as i64 - 1).max(0) as usize;
        self.pad_trim[i].0 = inp;
    }

    /// Non-destructively move pad `i`'s out-point by `delta` frames, clamped
    /// to `[in+1, clip_len]`.
    pub fn nudge_pad_out(&mut self, i: usize, delta: i64) {
        if i >= PADS {
            return;
        }
        let len = self.pad_clip_frames(i) as i64;
        let (inp, out) = self.pad_trim[i];
        let out = (out as i64 + delta).clamp(inp as i64 + 1, len.max(inp as i64 + 1)) as usize;
        self.pad_trim[i].1 = out;
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

    /// Pad `i`'s kind (one-shot / loop / scratch).
    pub fn pad_kind(&self, i: usize) -> PadKind {
        self.pad_kind.get(i).copied().unwrap_or_default()
    }

    /// Cycle pad `i`'s kind: OneShot → Loop → Scratch → …
    pub fn cycle_pad_kind(&mut self, i: usize) {
        if i < PADS {
            self.pad_kind[i] = match self.pad_kind[i] {
                PadKind::OneShot => PadKind::Loop,
                PadKind::Loop => PadKind::Scratch,
                PadKind::Scratch => PadKind::OneShot,
            };
        }
    }

    /// Pad `i`'s linear volume (1.0 = unity).
    pub fn pad_gain(&self, i: usize) -> f32 {
        self.pad_gain.get(i).copied().unwrap_or(1.0)
    }

    /// Nudge pad `i`'s volume by `delta`, clamped to `[0.0, 1.5]`.
    pub fn nudge_pad_gain(&mut self, i: usize, delta: f32) {
        if i < PADS {
            self.pad_gain[i] = (self.pad_gain[i] + delta).clamp(0.0, 1.5);
        }
    }

    /// Trigger pad `i`: start a new one-shot voice over its trimmed clip,
    /// summed onto the master bus. No-op if the pad is empty. Overlapping
    /// triggers stack (polyphonic).
    pub fn trigger_pad(&mut self, i: usize) {
        let (inp, out) = self.pad_trim.get(i).copied().unwrap_or((0, 0));
        if let Some(Some(clip)) = self.pads.get(i) {
            let total = clip.len() / 2;
            let in_f = inp.min(total);
            let len_f = out.min(total).saturating_sub(in_f);
            self.voices.push(SampleVoice {
                clip: Arc::clone(clip),
                pad: i,
                in_f,
                len_f,
                t: 0,
            });
        }
    }

    /// Number of sampler voices currently sounding.
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// Set the output sample rate (frames per second) for future tempo features.
    /// Called once at startup by the event loop.
    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate.max(1);
    }

    /// Render the next block: sum the active sampler voices into `out`
    /// (interleaved stereo), apply the master gain, and capture it when the
    /// recorder is armed. This is what the audio pump calls each block.
    pub fn fill_mix(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = 0.0;
        }
        self.mix_voices(out);
        self.apply(out);

        // Capture the final master output when armed (post-everything, so a
        // resample includes every playing pad — overdub).
        if self.recording {
            self.record_buf.extend_from_slice(out);
        }
    }

    /// Add each active voice's next block into `out` (interleaved stereo),
    /// advancing its position; voices that reach their end are removed.
    fn mix_voices(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        let gains = self.pad_gain;
        self.voices.retain_mut(|v| {
            let g = gains.get(v.pad).copied().unwrap_or(1.0);
            for i in 0..frames {
                match v.next_frame() {
                    Some((l, r)) => {
                        out[i * 2] += l * g;
                        out[i * 2 + 1] += r * g;
                    }
                    None => break,
                }
            }
            !v.done()
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
            "clip plays on the master bus"
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
        m.assign_pad(0, vec![0.3; 4096]);
        m.trigger_pad(0); // a pad voice plays → captured into the recording
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
            "captured the live pad audio"
        );
    }

    #[test]
    fn pad_trim_bounds_playback_non_destructively() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.5; 200]); // 100 frames; full trim by default
        assert_eq!(m.pad_trim(0), (0, 100));
        assert_eq!(m.pad_clip_frames(0), 100);
        m.nudge_pad_in(0, 20); // (20, 100)
        m.nudge_pad_out(0, -70); // (20, 30) → 10 frames
        assert_eq!(m.pad_trim(0), (20, 30));

        m.trigger_pad(0);
        m.fill_mix(&mut [0.0f32; 8]); // 4 of the 10 frames
        assert_eq!(m.active_voices(), 1);
        m.fill_mix(&mut [0.0f32; 100]); // drains the rest (< the buffer)
        assert_eq!(m.active_voices(), 0, "voice ends at the trim out-point");

        // The underlying clip is untouched — trimming only set bounds.
        assert_eq!(m.pad_clip_frames(0), 100);
    }

    #[test]
    fn pad_trim_clamps() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.0; 200]); // 100 frames
        m.nudge_pad_in(0, 1000); // can't reach/pass out
        assert_eq!(m.pad_trim(0).0, 99);
        m.nudge_pad_out(0, 1000); // can't pass the clip end
        assert_eq!(m.pad_trim(0).1, 100);
    }

    #[test]
    fn per_pad_volume_scales_that_pad() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![1.0; 64]);
        assert_eq!(m.pad_gain(0), 1.0);
        m.nudge_pad_gain(0, -0.5); // → 0.5
        m.trigger_pad(0);
        let mut buf = vec![0.0f32; 8];
        m.fill_mix(&mut buf);
        assert!(
            buf.iter().all(|&s| (s - 0.5).abs() < 1e-4),
            "pad gain applied"
        );
    }

    #[test]
    fn cycle_pad_kind_rotates() {
        let mut m = Mixer::new();
        assert_eq!(m.pad_kind(0), PadKind::OneShot);
        m.cycle_pad_kind(0);
        assert_eq!(m.pad_kind(0), PadKind::Loop);
        m.cycle_pad_kind(0);
        assert_eq!(m.pad_kind(0), PadKind::Scratch);
        m.cycle_pad_kind(0);
        assert_eq!(m.pad_kind(0), PadKind::OneShot, "wraps");
    }

    #[test]
    fn triggering_empty_pad_is_a_noop() {
        let mut m = Mixer::new();
        m.trigger_pad(2);
        assert_eq!(m.active_voices(), 0);
    }

    #[test]
    fn two_pads_sum_at_unity() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.3; 64]);
        m.assign_pad(1, vec![0.4; 64]);
        m.trigger_pad(0);
        m.trigger_pad(1);
        let mut buf = vec![0.0f32; 8];
        m.fill_mix(&mut buf);
        assert!(
            buf.iter().all(|&s| (s - 0.7).abs() < 1e-4),
            "pads sum on the master bus"
        );
    }
}
