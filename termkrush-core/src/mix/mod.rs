//! The mixer: the master bus over the sampler pads.
//!
//! It owns the sampler pads, sums their currently-sounding one-shot voices,
//! applies the **master gain**, and feeds the live-mix recorder. Pad types
//! (loop / scratch), tempo sync, and the arrangement render layer on top in
//! their own stories.

use std::sync::Arc;

/// Number of sampler pads (clip triggers).
pub const PADS: usize = 16;

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
        interp_frame(&self.clip, self.in_f as f64 + p.max(0.0))
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

/// Downsample an interleaved-stereo clip to `columns` `(min, max)` peak pairs
/// (mono-summed), for drawing a waveform.
fn clip_peaks(clip: &[f32], columns: usize) -> Vec<(f32, f32)> {
    let frames = clip.len() / 2;
    if frames == 0 || columns == 0 {
        return Vec::new();
    }
    let per = (frames as f64 / columns as f64).max(1.0);
    (0..columns)
        .map(|c| {
            let start = (c as f64 * per) as usize;
            let end = (((c + 1) as f64 * per) as usize).min(frames).max(start + 1);
            let (mut lo, mut hi) = (0.0f32, 0.0f32);
            for f in start..end {
                let s = 0.5 * (clip[f * 2] + clip[f * 2 + 1]);
                lo = lo.min(s);
                hi = hi.max(s);
            }
            (lo, hi)
        })
        .collect()
}

/// Linearly-interpolated stereo frame at absolute fractional frame `f`.
fn interp_frame(clip: &[f32], f: f64) -> (f32, f32) {
    let total = clip.len() / 2;
    if total == 0 {
        return (0.0, 0.0);
    }
    let f = f.clamp(0.0, (total - 1) as f64);
    let i0 = f.floor() as usize;
    let i1 = (i0 + 1).min(total - 1);
    let frac = (f - i0 as f64) as f32;
    (
        clip[i0 * 2] * (1.0 - frac) + clip[i1 * 2] * frac,
        clip[i0 * 2 + 1] * (1.0 - frac) + clip[i1 * 2 + 1] * frac,
    )
}

/// A scratch voice: walks a list of [`Stroke`]s (whip/wiki phrase), reading
/// the clip along each stroke and gating by its gain (the crossfader).
#[derive(Debug)]
struct ScratchVoice {
    clip: Arc<Vec<f32>>,
    pad: usize,
    strokes: Vec<crate::scratch::Stroke>,
    seg: usize,
    t: f64, // output frames into the current stroke
}

impl ScratchVoice {
    fn next_frame(&mut self) -> Option<(f32, f32)> {
        loop {
            let st = *self.strokes.get(self.seg)?;
            if self.t >= st.dur {
                self.seg += 1;
                self.t = 0.0;
                continue;
            }
            let frac = if st.dur > 0.0 { self.t / st.dur } else { 1.0 };
            let pos = st.from + (st.to - st.from) * frac;
            self.t += 1.0;
            let (l, r) = interp_frame(&self.clip, pos);
            return Some((l * st.gain, r * st.gain));
        }
    }

    fn done(&self) -> bool {
        self.seg >= self.strokes.len()
    }
}

/// The live scratch platter: a clip read at a controllable position + velocity.
/// Velocity is source frames per output frame — `1.0` ≈ normal forward (wiki),
/// negative = reverse (whip), `0.0` = stopped (silent, like a held platter).
/// The playhead persists across gestures, so a `<` then `>` continues from
/// wherever it stopped — exactly like a record under your hand.
#[derive(Debug)]
struct JogVoice {
    clip: Arc<Vec<f32>>,
    len: usize, // frames
    pos: f64,
    vel: f32,
}

impl JogVoice {
    fn next_frame(&mut self) -> (f32, f32) {
        if self.len == 0 || self.vel == 0.0 {
            return (0.0, 0.0);
        }
        let out = interp_frame(&self.clip, self.pos);
        self.pos = (self.pos + self.vel as f64).clamp(0.0, (self.len.saturating_sub(1)) as f64);
        out
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
    /// Per-pad scratch pivot (onset frame), computed on assign.
    pad_pivot: [usize; PADS],
    /// Per-pad recorded scratch phrase (sequence of whip/wiki units).
    pad_phrase: [Vec<crate::scratch::ScratchUnit>; PADS],
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
    /// Master pause: when set, `fill_mix` outputs silence and freezes every
    /// voice (positions held) so the whole mix can be paused and resumed.
    paused: bool,
    /// Master musical clock — frames elapsed, advanced each `fill_mix`.
    transport_frames: u64,
    /// Launch-quantize grid in beats (4.0 = one 4/4 bar — the default).
    quantize_beats: f32,
    /// Triggers held for their quantize boundary: `(pad, fire-at-frame)`.
    pending: Vec<(usize, u64)>,
    /// Currently-sounding one-shot voices, summed onto the master bus.
    voices: Vec<SampleVoice>,
    /// Currently-sounding scratch voices (whip/wiki phrases).
    scratch_voices: Vec<ScratchVoice>,
    /// A one-shot library preview at unity gain (not tied to a pad).
    preview: Option<SampleVoice>,
    /// The live scratch jog: a position-controlled platter over one clip.
    jog: Option<JogVoice>,
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
            pad_pivot: [0; PADS],
            pad_phrase: std::array::from_fn(|_| Vec::new()),
            pad_gain: [1.0; PADS],
            pad_env: [1.0; PADS],
            pad_env_target: [1.0; PADS],
            pad_fade: [1.0; PADS],
            master_bpm: None,
            global_speed: 1.0,
            paused: false,
            transport_frames: 0,
            quantize_beats: 4.0, // one bar
            pending: Vec::new(),
            voices: Vec::new(),
            scratch_voices: Vec::new(),
            preview: None,
            jog: None,
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
            self.pad_pivot[i] = crate::scratch::detect_pivot(&clip, 2);
            self.pads[i] = Some(Arc::new(clip));
            self.pad_trim[i] = (0, frames); // full clip by default
        }
    }

    /// Pad `i`'s trim bounds in frames `(in, out)`.
    pub fn pad_trim(&self, i: usize) -> (usize, usize) {
        self.pad_trim.get(i).copied().unwrap_or((0, 0))
    }

    /// Pad `i`'s **trimmed** clip region (interleaved stereo), for saving an
    /// edit back to disk. Empty if the pad has no clip.
    pub fn pad_clip_region(&self, i: usize) -> Vec<f32> {
        let (inp, out) = self.pad_trim(i);
        match self.pads.get(i).and_then(|p| p.as_ref()) {
            Some(clip) => {
                let total = clip.len() / 2;
                let (a, b) = ((inp.min(total)) * 2, (out.min(total)) * 2);
                clip.get(a..b).map(|s| s.to_vec()).unwrap_or_default()
            }
            None => Vec::new(),
        }
    }

    /// Downsample pad `i`'s whole clip to `columns` `(min, max)` peak pairs
    /// (mono-summed), for drawing a waveform. Empty if the pad has no clip.
    pub fn pad_peaks(&self, i: usize, columns: usize) -> Vec<(f32, f32)> {
        match self.pads.get(i).and_then(|p| p.as_ref()) {
            Some(clip) => clip_peaks(clip, columns),
            None => Vec::new(),
        }
    }

    /// `(min, max)` peak pairs over the scratch-platter clip, for its waveform.
    pub fn jog_peaks(&self, columns: usize) -> Vec<(f32, f32)> {
        match self.jog.as_ref() {
            Some(j) => clip_peaks(&j.clip, columns),
            None => Vec::new(),
        }
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

    /// Set pad `i`'s trim in-point to `frame` (clamped below the out-point).
    pub fn set_pad_trim_in(&mut self, i: usize, frame: usize) {
        if i < PADS {
            let out = self.pad_trim[i].1;
            self.pad_trim[i].0 = frame.min(out.saturating_sub(1));
        }
    }

    /// Set pad `i`'s trim out-point to `frame` (clamped above in, ≤ length).
    pub fn set_pad_trim_out(&mut self, i: usize, frame: usize) {
        if i < PADS {
            let len = self.pad_clip_frames(i);
            let inp = self.pad_trim[i].0;
            self.pad_trim[i].1 = frame.clamp(inp + 1, len);
        }
    }

    /// Stop every currently-sounding voice at once (keeps live pad play and
    /// arrangement playback from overlapping).
    pub fn clear_voices(&mut self) {
        self.voices.clear();
        self.scratch_voices.clear();
    }

    /// Play `samples` (interleaved stereo) once at unity gain as a library
    /// preview — replaces any prior preview, so it never stacks.
    pub fn preview(&mut self, samples: Vec<f32>) {
        let len_f = samples.len() / 2;
        self.preview = Some(SampleVoice {
            clip: Arc::new(samples),
            pad: 0,
            in_f: 0,
            len_f,
            pos: 0.0,
            speed: 1.0,
            looping: false,
        });
    }

    /// Stop the library preview, if any.
    pub fn stop_preview(&mut self) {
        self.preview = None;
    }

    /// Whether a library preview is currently sounding.
    pub fn is_previewing(&self) -> bool {
        self.preview.is_some()
    }

    // ---- live scratch jog ---------------------------------------------------

    /// Arm the scratch platter with `samples` (interleaved stereo). The playhead
    /// starts at the front, stopped. Replaces any prior jog clip.
    pub fn set_jog_source(&mut self, samples: Vec<f32>) {
        let len = samples.len() / 2;
        self.jog = Some(JogVoice {
            clip: Arc::new(samples),
            len,
            pos: 0.0,
            vel: 0.0,
        });
    }

    /// Remove the scratch platter (stops any jog sound).
    pub fn clear_jog(&mut self) {
        self.jog = None;
    }

    /// Whether a scratch platter is armed.
    pub fn has_jog(&self) -> bool {
        self.jog.is_some()
    }

    /// Set the jog velocity in source frames per output frame (signed; `0`
    /// stops/silences, negative reverses).
    pub fn set_jog_velocity(&mut self, vel: f32) {
        if let Some(j) = self.jog.as_mut() {
            j.vel = vel;
        }
    }

    /// Move the jog playhead to `frame` (clamped to the clip).
    pub fn set_jog_position(&mut self, frame: f64) {
        if let Some(j) = self.jog.as_mut() {
            j.pos = frame.clamp(0.0, j.len.saturating_sub(1) as f64);
        }
    }

    /// The jog playhead position in frames, if armed.
    pub fn jog_position(&self) -> Option<f64> {
        self.jog.as_ref().map(|j| j.pos)
    }

    /// The jog clip length in frames, if armed.
    pub fn jog_len(&self) -> usize {
        self.jog.as_ref().map(|j| j.len).unwrap_or(0)
    }

    /// Stop pad `i`'s voices (e.g. to toggle an audition off).
    pub fn stop_pad(&mut self, i: usize) {
        self.voices.retain(|v| v.pad != i);
        self.scratch_voices.retain(|v| v.pad != i);
    }

    /// Whether pad `i` currently has a sounding voice.
    pub fn pad_is_sounding(&self, i: usize) -> bool {
        self.voices.iter().any(|v| v.pad == i) || self.scratch_voices.iter().any(|v| v.pad == i)
    }

    /// Audition an explicit `[from, to)` region of pad `i` once, at native
    /// rate — stops any prior preview first. Used to hear right at a handle.
    pub fn audition_region(&mut self, i: usize, from: usize, to: usize) {
        self.voices.retain(|v| v.pad != i);
        self.scratch_voices.retain(|v| v.pad != i);
        if let Some(Some(clip)) = self.pads.get(i) {
            let total = clip.len() / 2;
            let in_f = from.min(total);
            let len_f = to.min(total).saturating_sub(in_f);
            if len_f == 0 {
                return;
            }
            self.voices.push(SampleVoice {
                clip: Arc::clone(clip),
                pad: i,
                in_f,
                len_f,
                pos: 0.0,
                speed: 1.0,
                looping: false,
            });
            self.pad_env[i] = 1.0;
            self.pad_env_target[i] = 1.0;
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

    /// Clear pad `i` back to empty: drop its clip, reset trim/kind/bpm/pivot/
    /// phrase/gain/envelope, and stop its voices.
    pub fn unload_pad(&mut self, i: usize) {
        if i >= PADS {
            return;
        }
        self.pads[i] = None;
        self.pad_trim[i] = (0, 0);
        self.pad_kind[i] = PadKind::OneShot;
        self.pad_bpm[i] = None;
        self.pad_pivot[i] = 0;
        self.pad_phrase[i].clear();
        self.pad_gain[i] = 1.0;
        self.pad_env[i] = 1.0;
        self.pad_env_target[i] = 1.0;
        self.voices.retain(|v| v.pad != i);
        self.scratch_voices.retain(|v| v.pad != i);
    }

    /// Pad `i`'s kind (one-shot / loop / scratch).
    pub fn pad_kind(&self, i: usize) -> PadKind {
        self.pad_kind.get(i).copied().unwrap_or_default()
    }

    /// Pad `i`'s scratch pivot (onset frame), found on assign.
    pub fn pad_pivot(&self, i: usize) -> usize {
        self.pad_pivot.get(i).copied().unwrap_or(0)
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

    /// Set pad `i`'s kind directly (the GUI picks a kind, not just cycles).
    pub fn set_pad_kind(&mut self, i: usize, kind: PadKind) {
        if i < PADS {
            self.pad_kind[i] = kind;
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

    /// Set pad `i`'s volume directly (for a slider), clamped to `[0.0, 1.5]`.
    pub fn set_pad_gain(&mut self, i: usize, gain: f32) {
        if i < PADS {
            self.pad_gain[i] = gain.clamp(0.0, 1.5);
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

    /// Master pause: freeze + silence the whole mix (voices hold position).
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Whether the mix is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Frames elapsed on the master clock.
    pub fn transport_frames(&self) -> u64 {
        self.transport_frames
    }

    /// The launch-quantize grid, in beats.
    pub fn quantize_beats(&self) -> f32 {
        self.quantize_beats
    }

    /// Set the launch-quantize grid in beats (e.g. 4 = bar, 1 = beat).
    pub fn set_quantize_beats(&mut self, beats: f32) {
        self.quantize_beats = beats.max(0.0);
    }

    /// Frames per beat at the master tempo, or 0 if no tempo is set yet.
    fn frames_per_beat(&self) -> f64 {
        match self.master_bpm {
            Some(b) if b > 0.0 => self.sample_rate as f64 * 60.0 / b as f64,
            _ => 0.0,
        }
    }

    /// Frames from now until the next bar line (0 if no tempo).
    pub fn frames_to_next_bar(&self) -> u64 {
        let bar = self.frames_per_beat() * 4.0;
        if bar <= 0.0 {
            return 0;
        }
        let pos = self.transport_frames as f64;
        ((pos / bar).ceil() * bar - pos).max(0.0).round() as u64
    }

    /// The next quantize-grid boundary at or after the current position.
    fn next_quant_boundary(&self) -> u64 {
        let q = self.frames_per_beat() * self.quantize_beats as f64;
        if q <= 0.0 {
            return self.transport_frames;
        }
        let pos = self.transport_frames as f64;
        ((pos / q).ceil() * q).round() as u64
    }

    /// Trigger pad `i` **launch-quantized** — start it on the next grid
    /// boundary so it never begins mid-bar. Fires immediately if no tempo is
    /// set or the position is already on a boundary. Pending until the
    /// boundary passes in `fill_mix`.
    pub fn trigger_quantized(&mut self, i: usize) {
        if self.frames_per_beat() <= 0.0 || self.quantize_beats <= 0.0 {
            self.trigger_pad(i);
            return;
        }
        let at = self.next_quant_boundary();
        if at <= self.transport_frames {
            self.trigger_pad(i);
        } else {
            self.pending.push((i, at));
        }
    }

    /// Number of triggers currently held for their quantize boundary.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
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

    /// Number of voices currently sounding (sample + scratch).
    pub fn active_voices(&self) -> usize {
        self.voices.len() + self.scratch_voices.len()
    }

    /// Play a **whip** scratch on pad `i` (around its pivot).
    pub fn scratch_whip(&mut self, i: usize) {
        self.push_scratch(i, crate::scratch::whip);
    }

    /// Play a **wiki** scratch on pad `i`.
    pub fn scratch_wiki(&mut self, i: usize) {
        self.push_scratch(i, crate::scratch::wiki);
    }

    fn push_scratch(&mut self, i: usize, build: fn(usize, usize) -> Vec<crate::scratch::Stroke>) {
        let (slice, pivot) = (self.scratch_slice(), self.pad_pivot(i));
        if let Some(Some(clip)) = self.pads.get(i) {
            let strokes = build(pivot, slice);
            self.push_scratch_voice(i, Arc::clone(clip), strokes);
        }
    }

    /// Half-rub length in frames: a sixteenth of the master beat when known,
    /// else ~80 ms — so phrases lock to the grid.
    fn scratch_slice(&self) -> usize {
        match self.master_bpm {
            Some(b) if b > 0.0 => ((self.sample_rate as f64 * 60.0 / b as f64) / 4.0) as usize,
            _ => (self.sample_rate as f64 * 0.08) as usize,
        }
        .max(1)
    }

    fn push_scratch_voice(
        &mut self,
        i: usize,
        clip: Arc<Vec<f32>>,
        strokes: Vec<crate::scratch::Stroke>,
    ) {
        self.scratch_voices.push(ScratchVoice {
            clip,
            pad: i,
            strokes,
            seg: 0,
            t: 0.0,
        });
        self.pad_env[i] = 1.0;
        self.pad_env_target[i] = 1.0;
    }

    /// Append a unit to pad `i`'s scratch phrase.
    pub fn push_phrase(&mut self, i: usize, unit: crate::scratch::ScratchUnit) {
        if i < PADS {
            self.pad_phrase[i].push(unit);
        }
    }

    /// Clear pad `i`'s scratch phrase.
    pub fn clear_phrase(&mut self, i: usize) {
        if i < PADS {
            self.pad_phrase[i].clear();
        }
    }

    /// Number of units in pad `i`'s scratch phrase.
    pub fn pad_phrase_len(&self, i: usize) -> usize {
        self.pad_phrase.get(i).map(|p| p.len()).unwrap_or(0)
    }

    /// Pad `i`'s phrase as direction glyphs: `>` wiki (forward), `<` whip (back).
    pub fn pad_phrase_glyphs(&self, i: usize) -> String {
        self.pad_phrase
            .get(i)
            .map(|p| {
                p.iter()
                    .map(|u| match u {
                        crate::scratch::ScratchUnit::Wiki => '>',
                        crate::scratch::ScratchUnit::Whip => '<',
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Play pad `i`'s recorded phrase (or a single wiki if it's empty).
    pub fn play_phrase(&mut self, i: usize) {
        let (slice, pivot) = (self.scratch_slice(), self.pad_pivot(i));
        let strokes = if self.pad_phrase_len(i) > 0 {
            crate::scratch::phrase_strokes(&self.pad_phrase[i], pivot, slice)
        } else {
            crate::scratch::wiki(pivot, slice)
        };
        if let Some(Some(clip)) = self.pads.get(i) {
            let clip = Arc::clone(clip);
            self.push_scratch_voice(i, clip, strokes);
        }
    }

    /// Set the output sample rate (frames per second) for future tempo features.
    /// Called once at startup by the event loop.
    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate.max(1);
    }

    /// The output sample rate (frames per second).
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Render the next block: sum the active sampler voices into `out`
    /// (interleaved stereo), apply the master gain, and capture it when the
    /// recorder is armed. This is what the audio pump calls each block.
    pub fn fill_mix(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = 0.0;
        }
        // Master pause: silence + freeze (don't advance voices or record).
        if self.paused {
            return;
        }
        // Release any launch-quantized triggers whose boundary lands in (or
        // before) this buffer, then advance the master clock past it.
        let frames = (out.len() / 2) as u64;
        if self.frames_per_beat() > 0.0 && !self.pending.is_empty() {
            let horizon = self.transport_frames + frames;
            let mut due: Vec<usize> = Vec::new();
            self.pending.retain(|&(pad, at)| {
                if at < horizon {
                    due.push(pad);
                    false
                } else {
                    true
                }
            });
            for pad in due {
                self.trigger_pad(pad);
            }
        }
        self.transport_frames += frames;
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
        // Scratch voices sum on top, gated by the same pad gain/envelope.
        self.scratch_voices.retain_mut(|v| {
            let env = envs.get(v.pad).copied().unwrap_or(1.0);
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
        // The library preview sums on top at unity gain (no pad gain/envelope).
        if let Some(v) = self.preview.as_mut() {
            for i in 0..frames {
                match v.next_frame() {
                    Some((l, r)) => {
                        out[i * 2] += l;
                        out[i * 2 + 1] += r;
                    }
                    None => break,
                }
            }
            if v.done() {
                self.preview = None;
            }
        }
        // The scratch platter sums on top at unity gain — it sounds only while
        // the platter is moving (velocity != 0).
        if let Some(j) = self.jog.as_mut() {
            for i in 0..frames {
                let (l, r) = j.next_frame();
                out[i * 2] += l;
                out[i * 2 + 1] += r;
            }
        }
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
    fn pad_peaks_downsamples_to_columns() {
        let mut m = Mixer::new();
        assert!(m.pad_peaks(0, 32).is_empty(), "empty pad has no peaks");
        // A clip that ramps 0..1 across 1000 frames.
        let clip: Vec<f32> = (0..1000)
            .flat_map(|i| [i as f32 / 1000.0, i as f32 / 1000.0])
            .collect();
        m.assign_pad(0, clip);
        let peaks = m.pad_peaks(0, 50);
        assert_eq!(peaks.len(), 50);
        // Peaks rise left-to-right; the last column's max exceeds the first's.
        assert!(peaks.last().unwrap().1 > peaks.first().unwrap().1);
        for (lo, hi) in &peaks {
            assert!(lo <= hi);
        }
    }

    #[test]
    fn jog_moves_and_sounds_only_while_spinning() {
        let mut m = Mixer::new();
        // A 1000-frame ramp clip so interpolation has something to read.
        let clip: Vec<f32> = (0..1000).flat_map(|i| [i as f32 / 1000.0, 0.0]).collect();
        m.set_jog_source(clip);
        assert!(m.has_jog());
        assert_eq!(m.jog_len(), 1000);
        assert_eq!(m.jog_position(), Some(0.0));

        // Stopped: silent, playhead doesn't move.
        let mut buf = vec![0.0f32; 128 * 2];
        m.fill_mix(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0), "still platter is silent");
        assert_eq!(m.jog_position(), Some(0.0));

        // Forward (wiki): advances ~1 frame per output frame and sounds.
        m.set_jog_velocity(1.0);
        let mut buf = vec![0.0f32; 64 * 2];
        m.fill_mix(&mut buf);
        assert!(buf.iter().any(|&s| s != 0.0), "spinning forward sounds");
        let pos = m.jog_position().unwrap();
        assert!((pos - 64.0).abs() < 1.0, "advanced ~64 frames, got {pos}");

        // Reverse (whip): playhead goes back toward the start.
        m.set_jog_velocity(-2.0);
        m.fill_mix(&mut [0.0f32; 32]);
        assert!(m.jog_position().unwrap() < pos, "reverse moves backward");

        // Position clamps at the front.
        m.set_jog_position(0.0);
        m.set_jog_velocity(-5.0);
        m.fill_mix(&mut [0.0f32; 64]);
        assert_eq!(m.jog_position(), Some(0.0), "clamps at the front edge");

        m.clear_jog();
        assert!(!m.has_jog());
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
    fn phrase_records_and_plays() {
        use crate::scratch::ScratchUnit;
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        m.assign_pad(0, vec![0.5; 8000]);
        assert_eq!(m.pad_phrase_len(0), 0);
        m.push_phrase(0, ScratchUnit::Whip);
        m.push_phrase(0, ScratchUnit::Wiki);
        m.push_phrase(0, ScratchUnit::Whip);
        assert_eq!(m.pad_phrase_len(0), 3);
        m.play_phrase(0);
        assert_eq!(
            m.active_voices(),
            1,
            "the phrase plays as one scratch voice"
        );
        m.clear_phrase(0);
        assert_eq!(m.pad_phrase_len(0), 0);
        // An empty phrase still plays a single wiki (no crash).
        m.play_phrase(0);
        assert_eq!(m.active_voices(), 2);
    }

    #[test]
    fn scratch_whip_mutes_forward_sounds_backward() {
        let mut m = Mixer::new();
        m.set_sample_rate(1000); // ~80-frame rub slice
        m.assign_pad(0, vec![0.5; 800]); // 400 frames, flat 0.5
        m.scratch_whip(0);
        assert_eq!(m.active_voices(), 1);
        let mut buf = vec![0.0f32; 320]; // 160 frames = two 80-frame strokes
        m.fill_mix(&mut buf);
        assert!(buf[..160].iter().all(|&s| s.abs() < 1e-6), "forward muted");
        assert!(
            buf[160..].iter().any(|&s| (s - 0.5).abs() < 1e-4),
            "back sounds"
        );
        m.fill_mix(&mut [0.0f32; 8]);
        assert_eq!(m.active_voices(), 0, "scratch ends after its strokes");
    }

    #[test]
    fn scratch_wiki_sounds_throughout() {
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        m.assign_pad(0, vec![0.5; 800]);
        m.scratch_wiki(0);
        let mut buf = vec![0.0f32; 320];
        m.fill_mix(&mut buf);
        assert!(
            buf[..160].iter().any(|&s| (s - 0.5).abs() < 1e-4),
            "forward sounds"
        );
        assert!(
            buf[160..].iter().any(|&s| (s - 0.5).abs() < 1e-4),
            "back sounds"
        );
    }

    #[test]
    fn assign_finds_the_scratch_pivot() {
        let mut m = Mixer::new();
        let mut clip = vec![0.0f32; 2000]; // 1000 frames, burst at 500
        for f in 500..540 {
            clip[f * 2] = 0.8;
            clip[f * 2 + 1] = 0.8;
        }
        m.assign_pad(0, clip);
        assert!(
            (m.pad_pivot(0) as i64 - 500).abs() < 25,
            "pivot near the onset"
        );
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
    fn quantized_trigger_starts_on_the_next_bar() {
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        m.set_master_bpm(Some(120.0)); // beat = 500f, bar = 2000f
        m.assign_pad(0, vec![0.5; 40_000]);
        // Advance the clock to mid-bar (600 frames).
        m.fill_mix(&mut vec![0.0; 1200]);
        assert_eq!(m.transport_frames(), 600);
        // Trigger mid-bar → held, silent.
        m.trigger_quantized(0);
        assert_eq!(m.pending_count(), 1, "held for the bar line");
        assert_eq!(m.active_voices(), 0, "silent until the boundary");
        // Up to just before the bar (→ 1800): still held.
        m.fill_mix(&mut vec![0.0; 2400]);
        assert_eq!(m.pending_count(), 1);
        assert_eq!(m.active_voices(), 0);
        // Cross the bar (→ 2200): it starts.
        m.fill_mix(&mut vec![0.0; 800]);
        assert_eq!(m.pending_count(), 0, "released at the bar");
        assert_eq!(m.active_voices(), 1, "started on the bar line");
    }

    #[test]
    fn quantize_grid_and_frames_to_next_bar() {
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        m.set_master_bpm(Some(120.0)); // beat 500f, bar 2000f
        assert_eq!(m.quantize_beats(), 4.0, "default grid is one bar");
        assert_eq!(
            m.frames_to_next_bar(),
            0,
            "at the start we're on a bar line"
        );
        m.fill_mix(&mut vec![0.0; 1200]); // advance 600 frames
        assert_eq!(m.frames_to_next_bar(), 1400, "1400 frames to the 2000 line");
        m.set_quantize_beats(1.0); // one-beat grid
        assert_eq!(m.quantize_beats(), 1.0);
    }

    #[test]
    fn on_grid_trigger_and_no_tempo_fire_immediately() {
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        m.assign_pad(0, vec![0.5; 4000]);
        // No tempo → immediate.
        m.trigger_quantized(0);
        assert_eq!(m.active_voices(), 1, "no tempo fires now");
        assert_eq!(m.pending_count(), 0);
        // With tempo, exactly on a bar line (frame 0) → immediate.
        let mut m = Mixer::new();
        m.set_sample_rate(1000);
        m.set_master_bpm(Some(120.0));
        m.assign_pad(0, vec![0.5; 4000]);
        m.trigger_quantized(0); // transport at 0 = a boundary
        assert_eq!(m.active_voices(), 1, "on-grid fires now");
    }

    #[test]
    fn master_pause_freezes_and_silences_then_resumes() {
        let mut m = Mixer::new();
        m.assign_pad(0, vec![0.5; 40_000]);
        m.trigger_pad(0);
        let mut buf = vec![0.0f32; 8];
        m.fill_mix(&mut buf);
        assert!(buf.iter().all(|&s| (s - 0.5).abs() < 1e-4), "playing");
        // Pause → silence, and the voice is frozen (still present, not advanced).
        m.set_paused(true);
        let mut buf = vec![0.5f32; 8];
        m.fill_mix(&mut buf);
        assert!(buf.iter().all(|&s| s == 0.0), "paused is silent");
        assert_eq!(m.active_voices(), 1, "voice frozen, not dropped");
        // Resume → sound returns from where it was.
        m.set_paused(false);
        let mut buf = vec![0.0f32; 8];
        m.fill_mix(&mut buf);
        assert!(buf.iter().all(|&s| (s - 0.5).abs() < 1e-4), "resumed");
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
