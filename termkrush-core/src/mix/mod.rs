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

/// Number of sampler pads (clip triggers).
pub const PADS: usize = 7;

/// A clip playback pattern — how a triggered pad reads its clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pattern {
    /// Play the trimmed region forward once.
    #[default]
    Straight,
    /// "over and silent": forward, but gated on/off at the beat division —
    /// rhythmic cutting.
    Cut,
    /// "over and back": a read head bounces across a short slice, audible
    /// both ways — a baby scratch.
    BabyScratch,
    /// Forward, gated on/off at a finer (sixteenth) division — the
    /// transformer chop.
    Transformer,
    /// Loop a tiny slice rapidly — a roll / stutter.
    Stutter,
    /// Forward with a sinusoidally wobbling read rate — pitch vibrato.
    Warble,
    /// Play the trimmed region backward.
    Reverse,
}

/// One playing sampler voice. Reads its clip according to a [`Pattern`],
/// beat-synced via `div` (the beat-division length in frames).
#[derive(Debug)]
struct SampleVoice {
    clip: Arc<Vec<f32>>, // interleaved stereo at the mix rate
    pattern: Pattern,
    in_f: usize,  // trim in-point (frames)
    len_f: usize, // playable length in output frames
    t: usize,     // output frames elapsed
    div: f64,     // beat-division length in frames (cut gate / scratch slice)
    head: f64,    // baby-scratch read head (absolute frame, fractional)
    dir: f64,     // baby-scratch direction (+1 / -1)
}

impl SampleVoice {
    /// Interpolated stereo frame at fractional frame index `f` (clamped).
    fn frame_at(&self, f: f64) -> (f32, f32) {
        let total = self.clip.len() / 2;
        if total == 0 {
            return (0.0, 0.0);
        }
        let p0 = (f.floor() as usize).min(total - 1);
        let p1 = (p0 + 1).min(total - 1);
        let frac = (f - f.floor()) as f32;
        let (a, b) = (p0 * 2, p1 * 2);
        (
            self.clip[a] * (1.0 - frac) + self.clip[b] * frac,
            self.clip[a + 1] * (1.0 - frac) + self.clip[b + 1] * frac,
        )
    }

    /// The next output frame, or `None` once the voice is finished.
    fn next_frame(&mut self) -> Option<(f32, f32)> {
        if self.t >= self.len_f {
            return None;
        }
        let out = match self.pattern {
            Pattern::Straight => self.frame_at((self.in_f + self.t) as f64),
            Pattern::Cut => {
                let (l, r) = self.frame_at((self.in_f + self.t) as f64);
                let gate = if (self.t as f64 % self.div) < self.div / 2.0 {
                    1.0
                } else {
                    0.0
                };
                (l * gate, r * gate)
            }
            Pattern::BabyScratch => {
                let frame = self.frame_at(self.head);
                let lo = self.in_f as f64;
                let hi = (self.in_f as f64 + self.div).min((self.clip.len() / 2) as f64 - 1.0);
                self.head += self.dir;
                if self.head >= hi {
                    self.head = hi;
                    self.dir = -1.0;
                } else if self.head <= lo {
                    self.head = lo;
                    self.dir = 1.0;
                }
                frame
            }
            Pattern::Transformer => {
                let (l, r) = self.frame_at((self.in_f + self.t) as f64);
                let cell = (self.div / 2.0).max(1.0); // sixteenth: finer than Cut
                let gate = if (self.t as f64 % cell) < cell / 2.0 {
                    1.0
                } else {
                    0.0
                };
                (l * gate, r * gate)
            }
            Pattern::Stutter => {
                let slice = (self.div / 2.0).max(1.0) as usize; // tiny looping slice
                self.frame_at((self.in_f + self.t % slice) as f64)
            }
            Pattern::Warble => {
                // Wobble the read rate ±30% sinusoidally for pitch vibrato.
                let phase = std::f64::consts::TAU * self.t as f64 / self.div;
                let frame = self.frame_at(self.head);
                self.head += 1.0 + 0.3 * phase.sin();
                frame
            }
            Pattern::Reverse => {
                let idx = self.in_f + self.len_f.saturating_sub(1) - self.t;
                self.frame_at(idx as f64)
            }
        };
        self.t += 1;
        Some(out)
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
    /// Output sample rate, so beat-synced pad patterns convert seconds→frames.
    sample_rate: u32,
    /// Clip assigned to each sampler pad (interleaved stereo), if any.
    pads: [Option<Arc<Vec<f32>>>; PADS],
    /// Manually-set BPM per pad (for later beat-sync / auto-bpm).
    pad_bpm: [Option<f32>; PADS],
    /// Non-destructive trim bounds per pad, in frames `(in, out)`. The clip
    /// samples are never modified; triggering plays only `[in, out)`.
    pad_trim: [(usize, usize); PADS],
    /// Playback pattern per pad (Straight / Cut / BabyScratch).
    pad_pattern: [Pattern; PADS],
    /// Per-pad auto-BPM: when on, triggering time-stretches the clip to a
    /// target tempo (beat-match) instead of playing at its native rate.
    pad_autobpm: [bool; PADS],
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
    /// A master bus at unity gain with empty pads.
    pub fn new() -> Self {
        Mixer {
            master: 1.0,
            smoothed: 1.0,
            sample_rate: 44_100,
            pads: Default::default(),
            pad_bpm: Default::default(),
            pad_trim: Default::default(),
            pad_pattern: Default::default(),
            pad_autobpm: Default::default(),
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

    /// Trigger pad `i`: start a new one-shot voice from the start of its
    /// clip, summed atop whatever the decks are playing. No-op if the pad
    /// is empty. Overlapping triggers stack (polyphonic).
    pub fn trigger_pad(&mut self, i: usize) {
        let (inp, out) = self.pad_trim.get(i).copied().unwrap_or((0, 0));
        let pattern = self.pad_pattern.get(i).copied().unwrap_or_default();
        // Beat division (eighth note) in frames, from the pad's BPM; fall
        // back to ~1/8 s when no tempo is known.
        let div = match self.pad_bpm.get(i).copied().flatten() {
            Some(bpm) if bpm > 0.0 => (self.sample_rate as f64 * 60.0 / bpm as f64) / 2.0,
            _ => self.sample_rate as f64 * 0.125,
        };
        if let Some(Some(clip)) = self.pads.get(i) {
            let total = clip.len() / 2;
            let in_f = inp.min(total);
            let len_f = out.min(total).saturating_sub(in_f);
            self.voices.push(SampleVoice {
                clip: Arc::clone(clip),
                pattern,
                in_f,
                len_f,
                t: 0,
                div: div.max(1.0),
                head: in_f as f64,
                dir: 1.0,
            });
        }
    }

    /// Cycle pad `i`'s playback pattern: Straight → Cut → BabyScratch → …
    pub fn cycle_pad_pattern(&mut self, i: usize) {
        if i < PADS {
            self.pad_pattern[i] = match self.pad_pattern[i] {
                Pattern::Straight => Pattern::Cut,
                Pattern::Cut => Pattern::BabyScratch,
                Pattern::BabyScratch => Pattern::Transformer,
                Pattern::Transformer => Pattern::Stutter,
                Pattern::Stutter => Pattern::Warble,
                Pattern::Warble => Pattern::Reverse,
                Pattern::Reverse => Pattern::Straight,
            };
        }
    }

    /// Pad `i`'s current playback pattern.
    pub fn pad_pattern(&self, i: usize) -> Pattern {
        self.pad_pattern.get(i).copied().unwrap_or_default()
    }

    /// Toggle pad `i`'s auto-BPM (beat-match on trigger).
    pub fn toggle_pad_autobpm(&mut self, i: usize) {
        if i < PADS {
            self.pad_autobpm[i] = !self.pad_autobpm[i];
        }
    }

    /// Whether pad `i` is set to auto-BPM.
    pub fn pad_autobpm(&self, i: usize) -> bool {
        self.pad_autobpm.get(i).copied().unwrap_or(false)
    }

    /// Trigger pad `i`, beat-matching to `target_bpm` when auto-BPM is on:
    /// the trimmed region is time-stretched (pitch-preserving) by
    /// `clip_bpm / target_bpm` and played straight. Falls back to the normal
    /// patterned trigger when auto-BPM is off or the tempos are unknown.
    pub fn trigger_pad_synced(&mut self, i: usize, target_bpm: f32) {
        let auto = self.pad_autobpm.get(i).copied().unwrap_or(false);
        let src = self.pad_bpm.get(i).copied().flatten();
        if let (true, Some(src_bpm)) = (auto && target_bpm > 0.0, src) {
            let (in_f, out_f) = self.pad_trim.get(i).copied().unwrap_or((0, 0));
            if let Some(Some(clip)) = self.pads.get(i) {
                let total = clip.len() / 2;
                let (a, b) = ((in_f.min(total)) * 2, (out_f.min(total)) * 2);
                if b > a {
                    let stretched =
                        crate::audio::time_stretch(&clip[a..b], 2, src_bpm / target_bpm);
                    let len_f = stretched.len() / 2;
                    self.voices.push(SampleVoice {
                        clip: Arc::new(stretched),
                        pattern: Pattern::Straight,
                        in_f: 0,
                        len_f,
                        t: 0,
                        div: 1.0,
                        head: 0.0,
                        dir: 1.0,
                    });
                    return;
                }
            }
        }
        self.trigger_pad(i); // native rate, with the pad's pattern
    }

    /// Number of sampler voices currently sounding.
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// Set the output sample rate so beat-synced pad patterns know how many
    /// frames a second is. Called once at startup by the event loop.
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
        self.voices.retain_mut(|v| {
            for i in 0..frames {
                match v.next_frame() {
                    Some((l, r)) => {
                        out[i * 2] += l;
                        out[i * 2 + 1] += r;
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
    fn cycle_pad_pattern_rotates_through_all() {
        let mut m = Mixer::new();
        let order = [
            Pattern::Straight,
            Pattern::Cut,
            Pattern::BabyScratch,
            Pattern::Transformer,
            Pattern::Stutter,
            Pattern::Warble,
            Pattern::Reverse,
        ];
        for &want in &order {
            assert_eq!(m.pad_pattern(0), want);
            m.cycle_pad_pattern(0);
        }
        assert_eq!(m.pad_pattern(0), Pattern::Straight, "wraps");
    }

    /// Build a rising-ramp stereo clip (frame value = i/n).
    fn ramp_clip(n: usize) -> Vec<f32> {
        let mut c = Vec::with_capacity(n * 2);
        for i in 0..n {
            let v = i as f32 / n as f32;
            c.push(v);
            c.push(v);
        }
        c
    }

    #[test]
    fn reverse_pattern_plays_backward() {
        let mut m = Mixer::new();
        m.assign_pad(0, ramp_clip(200));
        // Straight → ... → Reverse (6 cycles).
        for _ in 0..6 {
            m.cycle_pad_pattern(0);
        }
        assert_eq!(m.pad_pattern(0), Pattern::Reverse);
        m.trigger_pad(0);
        let mut buf = vec![0.0f32; 400];
        m.fill_mix(&mut buf);
        let left: Vec<f32> = (0..200).map(|i| buf[i * 2]).collect();
        assert!(
            left[0] > left[199],
            "starts high (end of clip), ends low (start)"
        );
    }

    #[test]
    fn stutter_pattern_loops_a_short_slice() {
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        m.assign_pad(0, ramp_clip(800));
        m.set_pad_bpm(0, Some(120.0)); // div=250 → slice = 125 frames
        for _ in 0..4 {
            m.cycle_pad_pattern(0);
        } // → Stutter
        assert_eq!(m.pad_pattern(0), Pattern::Stutter);
        m.trigger_pad(0);
        let mut buf = vec![0.0f32; 1600];
        m.fill_mix(&mut buf);
        // The slice repeats: frame 0 and frame 125 read the same source.
        assert!((buf[0] - buf[125 * 2]).abs() < 1e-6, "slice loops");
    }

    #[test]
    fn cut_pattern_gates_the_clip_on_the_beat() {
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        m.assign_pad(0, vec![0.5; 1200]); // 600 frames, flat 0.5
        m.set_pad_bpm(0, Some(120.0)); // div = 250 frames → 125 on / 125 off
        m.cycle_pad_pattern(0); // Straight → Cut
        m.trigger_pad(0);
        let mut buf = vec![0.0f32; 1200]; // 600 frames
        m.fill_mix(&mut buf);
        assert!(
            buf.iter().any(|&s| s.abs() < 1e-6),
            "off-beats gated to silence"
        );
        assert!(
            buf.iter().any(|&s| (s - 0.5).abs() < 1e-6),
            "on-beats pass through"
        );
    }

    #[test]
    fn baby_scratch_reverses_over_a_slice() {
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        // Ramp clip: value rises with frame, so a reversal shows as output
        // rising then falling.
        let n = 400usize;
        let mut clip = Vec::with_capacity(n * 2);
        for i in 0..n {
            let v = i as f32 / n as f32;
            clip.push(v);
            clip.push(v);
        }
        m.assign_pad(0, clip);
        m.set_pad_bpm(0, Some(120.0)); // 250-frame scratch slice
        m.cycle_pad_pattern(0);
        m.cycle_pad_pattern(0); // → BabyScratch
        m.trigger_pad(0);
        let mut buf = vec![0.0f32; 800]; // 400 frames
        m.fill_mix(&mut buf);
        let left: Vec<f32> = (0..400).map(|i| buf[i * 2]).collect();
        let peak = left
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert!(peak < 399, "head reverses before the end (peak mid-buffer)");
        assert!(
            left[399] < left[peak],
            "output comes back down after reversal"
        );
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
    fn auto_bpm_time_stretches_a_pad_on_trigger() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.3; 8192]); // 4096 frames
        m.set_pad_bpm(0, Some(120.0));
        m.toggle_pad_autobpm(0);
        assert!(m.pad_autobpm(0));
        m.trigger_pad_synced(0, 60.0); // half the tempo → ~2× length
        assert_eq!(m.active_voices(), 1);
        m.fill_mix(&mut [0.0f32; 8192]); // 4096 frames — the NATIVE length
        assert_eq!(
            m.active_voices(),
            1,
            "stretched clip outlasts its native length"
        );
        m.fill_mix(&mut [0.0f32; 9000]); // drain the rest
        assert_eq!(m.active_voices(), 0);
    }

    #[test]
    fn auto_bpm_off_plays_native_length() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.3; 200]); // 100 frames
        m.set_pad_bpm(0, Some(120.0)); // autobpm OFF
        m.trigger_pad_synced(0, 60.0);
        m.fill_mix(&mut [0.0f32; 200]); // 100 frames drains a native voice
        assert_eq!(
            m.active_voices(),
            0,
            "played at native length, not stretched"
        );
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
