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
    len_f: usize,        // playable length in source frames
    pos: f64,            // fractional read offset within the region
    speed: f64,          // read rate (1.0 = native; loop = bpm/master)
    looping: bool,       // repeat the region instead of stopping at the end
}

impl SampleVoice {
    /// Linearly-interpolated stereo frame at fractional region offset `p`.
    fn frame_at(&self, p: f64) -> (f32, f32) {
        let total = self.clip.len() / 2;
        if total == 0 {
            return (0.0, 0.0);
        }
        let base = (self.in_f as f64 + p).max(0.0);
        let i0 = (base.floor() as usize).min(total - 1);
        let i1 = (i0 + 1).min(total - 1);
        let frac = (base - base.floor()) as f32;
        (
            self.clip[i0 * 2] * (1.0 - frac) + self.clip[i1 * 2] * frac,
            self.clip[i0 * 2 + 1] * (1.0 - frac) + self.clip[i1 * 2 + 1] * frac,
        )
    }

    /// The next output frame, or `None` once finished. A looping voice wraps
    /// and never finishes on its own (stopped by deactivating its pad).
    fn next_frame(&mut self) -> Option<(f32, f32)> {
        if self.len_f == 0 {
            return None;
        }
        if self.pos >= self.len_f as f64 {
            if self.looping {
                self.pos %= self.len_f as f64;
            } else {
                return None;
            }
        }
        let out = self.frame_at(self.pos);
        self.pos += self.speed;
        Some(out)
    }

    fn done(&self) -> bool {
        !self.looping && self.pos >= self.len_f as f64
    }
}

/// Allowed master gain range: silence up to +3.5 dB of headroom.
pub const MASTER_MIN: f32 = 0.0;
pub const MASTER_MAX: f32 = 1.5;

/// Max change in applied master gain per frame, so master moves de-zipper.
const MASTER_RAMP_STEP: f32 = 1.0 / 512.0;

/// Per-block step for a *soft* pad activation fade (~0.3 s at 256-frame
/// blocks). A hard cut uses a step of 1.0 (one block).
const ENV_SOFT_STEP: f32 = 0.02;

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
    /// Per-pad activation envelope (0 = off … 1 = on), ramped toward target.
    pad_env: [f32; PADS],
    pad_env_target: [f32; PADS],
    /// Per-pad fade step per block (1.0 = hard cut, small = soft fade).
    pad_fade: [f32; PADS],
    /// The project's master tempo (BPM), seeded by the first loop. Loops sync
    /// to this; `None` until a loop with a known tempo plays.
    master_bpm: Option<f32>,
    /// Global speed multiplier — scales every loop together (1.0 = nominal).
    global_speed: f32,
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
            pad_env: [1.0; PADS],
            pad_env_target: [1.0; PADS],
            pad_fade: [1.0; PADS],
            master_bpm: None,
            global_speed: 1.0,
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

    /// Activate or deactivate pad `i`. `soft` ramps the envelope (a fade);
    /// hard snaps. A pad that fades fully off has its voices dropped.
    pub fn set_pad_active(&mut self, i: usize, active: bool, soft: bool) {
        if i < PADS {
            self.pad_env_target[i] = if active { 1.0 } else { 0.0 };
            self.pad_fade[i] = if soft { ENV_SOFT_STEP } else { 1.0 };
            if !soft {
                self.pad_env[i] = self.pad_env_target[i];
            }
        }
    }

    /// Whether pad `i` is active (its envelope target is on).
    pub fn pad_active(&self, i: usize) -> bool {
        self.pad_env_target.get(i).copied().unwrap_or(1.0) > 0.5
    }

    /// Pad `i`'s current envelope level (0..1).
    pub fn pad_env(&self, i: usize) -> f32 {
        self.pad_env.get(i).copied().unwrap_or(1.0)
    }

    /// Trigger pad `i`: start a new one-shot voice over its trimmed clip,
    /// summed onto the master bus. No-op if the pad is empty. Overlapping
    /// triggers stack (polyphonic).
    pub fn trigger_pad(&mut self, i: usize) {
        let (inp, out) = self.pad_trim.get(i).copied().unwrap_or((0, 0));
        let looping = self.pad_kind(i) == PadKind::Loop;
        // A loop pad holds a single voice — re-triggering restarts it.
        if looping {
            self.voices.retain(|v| v.pad != i);
            // The first loop with a known tempo seeds the master tempo.
            if self.master_bpm.is_none() {
                if let Some(bpm) = self.pad_bpm(i) {
                    if bpm > 0.0 {
                        self.master_bpm = Some(bpm);
                    }
                }
            }
        }
        // Loops varispeed to the master tempo (pitch rides); others native.
        let speed = self.pad_play_speed(i, looping);
        if let Some(Some(clip)) = self.pads.get(i) {
            let total = clip.len() / 2;
            let in_f = inp.min(total);
            let len_f = out.min(total).saturating_sub(in_f);
            self.voices.push(SampleVoice {
                clip: Arc::clone(clip),
                pad: i,
                in_f,
                len_f,
                pos: 0.0,
                speed,
                looping,
            });
            // Triggering implies the pad is on (hard) so it always sounds.
            self.pad_env[i] = 1.0;
            self.pad_env_target[i] = 1.0;
        }
    }

    /// Playback speed for pad `i`. A loop with a known tempo and a known
    /// master tempo varispeeds by `pad_bpm / master_bpm` so its beats lock to
    /// the grid (pitch rides); everything else plays native (1.0).
    fn pad_play_speed(&self, i: usize, looping: bool) -> f64 {
        if !looping {
            return 1.0;
        }
        let gs = self.global_speed as f64;
        match (self.pad_bpm(i), self.master_bpm) {
            (Some(pad), Some(master)) if pad > 0.0 && master > 0.0 => (pad / master) as f64 * gs,
            _ => gs, // looping but un-tempo'd: still follows the global speed
        }
    }

    /// The project's master tempo (BPM), if set (seeded by the first loop).
    pub fn master_bpm(&self) -> Option<f32> {
        self.master_bpm
    }

    /// The effective master tempo after the global speed multiplier.
    pub fn effective_bpm(&self) -> Option<f32> {
        self.master_bpm.map(|b| b * self.global_speed)
    }

    /// The global speed multiplier (1.0 = nominal).
    pub fn global_speed(&self) -> f32 {
        self.global_speed
    }

    /// Set the global speed (clamped 0.25–4.0); every playing loop re-syncs.
    pub fn set_global_speed(&mut self, s: f32) {
        self.global_speed = s.clamp(0.25, 4.0);
        let speeds: Vec<f64> = self
            .voices
            .iter()
            .map(|v| self.pad_play_speed(v.pad, v.looping))
            .collect();
        for (v, sp) in self.voices.iter_mut().zip(speeds) {
            v.speed = sp;
        }
    }

    /// Nudge the global speed by `delta`.
    pub fn nudge_global_speed(&mut self, delta: f32) {
        self.set_global_speed(self.global_speed + delta);
    }

    /// Override the master tempo (e.g. a manual set). Loops re-sync to it.
    pub fn set_master_bpm(&mut self, bpm: Option<f32>) {
        self.master_bpm = bpm.filter(|&b| b > 0.0);
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
        // Step each pad's activation envelope toward its target (per block).
        for i in 0..PADS {
            let t = self.pad_env_target[i];
            if self.pad_env[i] < t {
                self.pad_env[i] = (self.pad_env[i] + self.pad_fade[i]).min(t);
            } else if self.pad_env[i] > t {
                self.pad_env[i] = (self.pad_env[i] - self.pad_fade[i]).max(t);
            }
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
        let envs = self.pad_env;
        let targets = self.pad_env_target;
        self.voices.retain_mut(|v| {
            let env = envs.get(v.pad).copied().unwrap_or(1.0);
            // Drop voices on a pad that has fully faded off.
            if env <= 0.0 && targets.get(v.pad).copied().unwrap_or(1.0) <= 0.0 {
                return false;
            }
            let g = gains.get(v.pad).copied().unwrap_or(1.0) * env;
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
    fn global_speed_resyncs_all_playing_loops() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.5; 200]);
        m.set_pad_bpm(0, Some(120.0));
        m.cycle_pad_kind(0); // → Loop, seeds master 120 → speed 1.0
        m.trigger_pad(0);
        m.assign_pad(1, vec![0.5; 200]);
        m.set_pad_bpm(1, Some(120.0));
        m.cycle_pad_kind(1);
        m.trigger_pad(1);
        // Double the global speed → both loops re-sync to 2x; effective 240.
        m.set_global_speed(2.0);
        assert_eq!(m.global_speed(), 2.0);
        assert_eq!(m.effective_bpm(), Some(240.0));
        let mut buf = vec![0.0f32; 16];
        m.fill_mix(&mut buf);
        assert!(buf.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn loop_varispeeds_to_the_master_tempo() {
        let mut m = Mixer::new();
        // Ramp clip (value = i/100), 100 frames; loop at 100 BPM.
        let clip: Vec<f32> = (0..100).flat_map(|i| [i as f32 / 100.0; 2]).collect();
        m.assign_pad(0, clip);
        m.set_pad_bpm(0, Some(100.0));
        m.cycle_pad_kind(0); // → Loop
        m.set_master_bpm(Some(50.0)); // master half the clip's tempo → speed 2.0
        m.trigger_pad(0);
        let mut buf = vec![0.0f32; 100]; // 50 output frames
        m.fill_mix(&mut buf);
        // At speed 2.0, output frame 25 reads source frame 50 → value ~0.5
        // (native speed would read frame 25 → ~0.25).
        assert!(
            (buf[25 * 2] - 0.5).abs() < 0.02,
            "loop ran at ~2x, got {}",
            buf[25 * 2]
        );
    }

    #[test]
    fn first_loop_seeds_the_master_tempo() {
        let mut m = Mixer::new();
        assert_eq!(m.master_bpm(), None);
        // A one-shot with a tempo does NOT seed it.
        m.assign_pad(0, vec![0.5; 64]);
        m.set_pad_bpm(0, Some(100.0));
        m.trigger_pad(0);
        assert_eq!(m.master_bpm(), None, "only loops seed the master");
        // First loop does.
        m.assign_pad(1, vec![0.5; 64]);
        m.set_pad_bpm(1, Some(126.0));
        m.cycle_pad_kind(1); // → Loop
        m.trigger_pad(1);
        assert_eq!(m.master_bpm(), Some(126.0));
        // A later loop at a different tempo does not change it.
        m.assign_pad(2, vec![0.5; 64]);
        m.set_pad_bpm(2, Some(140.0));
        m.cycle_pad_kind(2);
        m.trigger_pad(2);
        assert_eq!(m.master_bpm(), Some(126.0), "first loop wins");
    }

    #[test]
    fn loop_pad_repeats_until_deactivated() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.5; 200]); // 100 frames
        m.cycle_pad_kind(0); // → Loop
        assert_eq!(m.pad_kind(0), PadKind::Loop);
        m.trigger_pad(0);
        // Play well past the 100-frame length — a one-shot would have ended.
        m.fill_mix(&mut [0.0f32; 1000]); // 500 frames
        assert_eq!(m.active_voices(), 1, "loop keeps going past its length");
        // Re-trigger doesn't stack a second loop voice.
        m.trigger_pad(0);
        assert_eq!(m.active_voices(), 1, "one loop voice per pad");
        // Deactivating stops it.
        m.set_pad_active(0, false, false);
        m.fill_mix(&mut [0.0f32; 8]);
        assert_eq!(m.active_voices(), 0, "loop stops when the pad is off");
    }

    #[test]
    fn hard_deactivate_silences_and_drops_voices() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.5; 40_000]);
        m.trigger_pad(0);
        m.set_pad_active(0, false, false); // hard off
        let mut buf = vec![0.0f32; 8];
        m.fill_mix(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0), "hard-off is silent");
        assert_eq!(m.active_voices(), 0, "voices dropped when off");
    }

    #[test]
    fn soft_deactivate_fades_gradually() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.5; 400_000]);
        m.trigger_pad(0);
        m.set_pad_active(0, false, true); // soft off
        m.fill_mix(&mut [0.0f32; 8]); // one block → env steps down, not to 0
        assert!(m.pad_env(0) > 0.0 && m.pad_env(0) < 1.0, "fading");
        assert_eq!(m.active_voices(), 1, "still sounding during the fade");
        assert!(!m.pad_active(0));
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
