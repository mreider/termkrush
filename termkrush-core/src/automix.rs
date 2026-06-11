//! The auto-mix engine: the naive render slice (see the 2026-06-11 pivot in
//! `.am/inception.md` and `docs/SPEC.md` §2).
//!
//! Input: the sequence (ordered tracks, repeats allowed) with each track's
//! decoded audio and tapped beat marks. Output: one continuous stereo mix.
//!
//! This slice implements the grammar's skeleton:
//! - **One master tempo** — the first entry's fitted tempo; every section
//!   varispeeds to it (pitch rides, like a platter).
//! - **Phrase sections** — each entry contributes 8–16 bars (clamped to
//!   what the track has), phrase-aligned to that track's own fitted grid;
//!   repeat entries pick different material.
//! - **Equal-loudness swaps** — sections are gain-matched to a shared RMS
//!   target and butt-joined on the master grid's phrase boundaries.
//! - **Determinism** — every choice flows from a seed hashed from the
//!   input itself (paths, order, marks). No wall clock, no thread order,
//!   no platform-dependent shortcuts: same input → the same `Vec<f32>`.
//!
//! Transition variety, scratches, drops, and the energy arc land in their
//! own stories on top of the `MixPlan` this module produces.

use std::sync::Arc;

use crate::beats::fit_grid;

/// The fixed render rate. Rendering is offline and must not depend on the
/// machine's output device, or the same sequence would produce different
/// mixes on different hardware.
pub const RENDER_RATE: u32 = 44_100;

/// Beats per bar. The grammar is 4/4 throughout.
pub const BEATS_PER_BAR: f64 = 4.0;

/// Shared loudness target every section is gain-matched to (linear RMS
/// over interleaved stereo, ≈ −15 dBFS). The reference mix's transitions
/// have a median level step of 0 dB — matching sections to one target is
/// what makes the default swap seamless.
pub const TARGET_RMS: f32 = 0.18;

/// Gain is clamped so a near-silent section can't be boosted into noise.
const MAX_GAIN: f32 = 4.0;

/// One track of input: decoded stereo audio at [`RENDER_RATE`] plus the
/// user's tapped beat marks (also at [`RENDER_RATE`]).
pub struct TrackInput {
    /// Identity used for seeding; typically the library path as a string.
    pub id: String,
    /// Interleaved stereo samples at [`RENDER_RATE`].
    pub samples: Arc<Vec<f32>>,
    /// Tapped beat marks in frames at [`RENDER_RATE`] (≥ 2 to render).
    pub beats: Vec<u64>,
}

/// How a section enters the mix. The reference's measured proportions:
/// the dominant move is the equal-loudness swap; about a quarter of
/// boundaries are hard cuts used as punctuation; ramped fades are rare
/// (~1 in 20). Every transition stays on the phrase boundary — the type
/// only shapes the incoming section's first moments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// Equal-loudness butt joint — the invisible default.
    Swap,
    /// Punctuation: the section punches in hot (≈ +6 dB) for its first
    /// beat, then settles to its matched gain over the next beat.
    Cut,
    /// The section ramps in from silence over a musical length.
    Fade {
        /// Ramp length in master beats (2, 4, or 8 — seeded).
        beats: u8,
    },
}

/// One planned section: which bars of which track land where in the mix.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Index into the planner's track list.
    pub track: usize,
    /// Position in the sequence this section realizes.
    pub order_pos: usize,
    /// First bar of the track used (in the track's own fitted grid).
    pub start_bar: u64,
    /// Section length in bars (8–16, clamped to what the track has).
    pub bars: u64,
    /// Output start frame (on a master phrase boundary by construction).
    pub out_start: u64,
    /// Output length in frames.
    pub out_frames: u64,
    /// Varispeed ratio: source frames consumed per output frame.
    pub speed: f64,
    /// Loudness-matching gain applied to the section.
    pub gain: f32,
    /// How this section enters (the first section always swaps in).
    pub transition: Transition,
}

/// One read-head move inside a flurry, in output frames relative to the
/// flurry's start. The whip/wiki grammar lives in the gain: a whip's
/// forward push is muted (gain 0) and only the pull-back sounds; a wiki
/// sounds both ways.
#[derive(Debug, Clone, PartialEq)]
pub struct ScratchStroke {
    /// Offset from the flurry start, output frames. Deliberately NOT
    /// quantized — the jitter here is what reads as a human hand.
    pub offset: f64,
    /// Stroke length in output frames.
    pub dur: f64,
    /// Source read start (track frames).
    pub from: f64,
    /// Source read end (track frames) — backwards when `to < from`.
    pub to: f64,
    /// Fader gain over the stroke (0 = muted push, 1 = audible).
    pub gain: f32,
}

/// A scratch flurry: 1–2 s of whip/wiki rubs over an onset-rich slice of
/// the track playing underneath, overlaid on the mix. Starts lock to the
/// beat grid (leaning on beat 2); the strokes inside stay loose.
#[derive(Debug, Clone, PartialEq)]
pub struct Flurry {
    /// Output start frame — always on a master beat.
    pub out_start: u64,
    /// Flurry length in output frames (~1–2 s, un-quantized).
    pub frames: u64,
    /// The track the slice is cut from (the one playing underneath).
    pub track: usize,
    /// Loudness-matching gain for the slice.
    pub gain: f32,
    pub strokes: Vec<ScratchStroke>,
}

/// A fader chop: a ~30–80 ms cut of the bed, placed at an un-quantized
/// offset inside a section (macro on the grid, micro human).
#[derive(Debug, Clone, PartialEq)]
pub struct Chop {
    pub out_start: u64,
    pub frames: u64,
}

/// The deterministic plan for a whole mix.
#[derive(Debug, Clone, PartialEq)]
pub struct MixPlan {
    /// Master tempo (the first entry's fitted tempo).
    pub master_bpm: f64,
    /// Master frames per bar at [`RENDER_RATE`].
    pub frames_per_bar: f64,
    /// The seed every choice flowed from (input-derived).
    pub seed: u64,
    pub sections: Vec<Section>,
    /// Engine-performed scratch passages, clustered into one stretch.
    pub flurries: Vec<Flurry>,
    /// Engine-performed fader chops.
    pub chops: Vec<Chop>,
}

impl MixPlan {
    /// Total mix length in frames.
    pub fn total_frames(&self) -> u64 {
        self.sections
            .last()
            .map(|s| s.out_start + s.out_frames)
            .unwrap_or(0)
    }
}

/// Why a plan could not be made.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The sequence has no entries.
    EmptySequence,
    /// An order entry points past the track list.
    BadOrderIndex(usize),
    /// A track has fewer than 2 beat marks (no grid can be fitted).
    NotEnoughBeats(usize),
    /// A track's marks fit no positive-interval grid.
    NoGrid(usize),
    /// A track is shorter than one bar of its own grid.
    TooShort(usize),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::EmptySequence => write!(f, "the sequence is empty"),
            PlanError::BadOrderIndex(i) => write!(f, "order entry {i} points past the track list"),
            PlanError::NotEnoughBeats(t) => write!(f, "track {t} needs at least 2 beat marks"),
            PlanError::NoGrid(t) => write!(f, "track {t}: beat marks fit no grid"),
            PlanError::TooShort(t) => write!(f, "track {t} is shorter than one bar"),
        }
    }
}

impl std::error::Error for PlanError {}

/// Per-track facts the planner derives once from the input.
struct TrackGrid {
    /// Fitted grid: first beat position (frames, f64) in the source.
    phase: f64,
    /// Fitted beat interval (frames).
    interval: f64,
    /// Whole bars available from `phase` to the end of the track.
    bars_avail: u64,
}

/// Build the deterministic plan: master tempo, section choices, gains,
/// and output positions. Pure — no I/O, no clock, no global state.
pub fn plan(tracks: &[TrackInput], order: &[usize]) -> Result<MixPlan, PlanError> {
    if order.is_empty() {
        return Err(PlanError::EmptySequence);
    }
    for (pos, &t) in order.iter().enumerate() {
        if t >= tracks.len() {
            return Err(PlanError::BadOrderIndex(pos));
        }
    }

    // Fit each track's grid once.
    let mut grids = Vec::with_capacity(tracks.len());
    for (ti, tr) in tracks.iter().enumerate() {
        if tr.beats.len() < 2 {
            return Err(PlanError::NotEnoughBeats(ti));
        }
        let (phase, interval) = fit_grid(&tr.beats).ok_or(PlanError::NoGrid(ti))?;
        let frames = (tr.samples.len() / 2) as f64;
        let bar = interval * BEATS_PER_BAR;
        let bars_avail = ((frames - phase) / bar).floor().max(0.0) as u64;
        if bars_avail == 0 {
            return Err(PlanError::TooShort(ti));
        }
        grids.push(TrackGrid {
            phase,
            interval,
            bars_avail,
        });
    }

    // Master tempo: the first entry in the sequence sets it.
    let first = &grids[order[0]];
    let master_bpm = RENDER_RATE as f64 * 60.0 / first.interval;
    let master_fpb = first.interval * BEATS_PER_BAR; // frames per master bar

    // The seed is the input: track ids, order, and every mark.
    let mut h = Fnv64::new();
    for tr in tracks {
        h.write(tr.id.as_bytes());
        for &b in &tr.beats {
            h.write_u64(b);
        }
    }
    for &o in order {
        h.write_u64(o as u64);
    }
    let seed = h.finish();
    let mut rng = SplitMix64::new(seed);

    // Section length distribution: 8–16 bars, even lengths (phrase feel),
    // matching the reference's 8–16-bar cluster. Longer stretches and the
    // occasional 24/32 arrive with the energy-arc story.
    const LENGTHS: [u64; 5] = [8, 10, 12, 14, 16];

    let mut sections = Vec::with_capacity(order.len());
    let mut used_starts: Vec<Vec<u64>> = vec![Vec::new(); tracks.len()];
    let mut out_bars: u64 = 0; // master bars laid down so far

    for (pos, &ti) in order.iter().enumerate() {
        let g = &grids[ti];
        let mut bars = LENGTHS[(rng.next() % LENGTHS.len() as u64) as usize];
        bars = bars.min(g.bars_avail); // short tracks contribute what they have

        // Material choice: a bar-aligned start inside the track. Repeat
        // entries land on different material when the track is long
        // enough — walk away from used starts deterministically.
        let span = g.bars_avail - bars; // last viable start bar
        let mut start_bar = if span == 0 {
            0
        } else {
            rng.next() % (span + 1)
        };
        if span > 0 {
            let stride = (span / 3).max(1);
            let mut tries = 0;
            while used_starts[ti].contains(&start_bar) && tries <= span {
                start_bar = (start_bar + stride) % (span + 1);
                tries += stride;
            }
        }
        used_starts[ti].push(start_bar);

        // Varispeed: source frames consumed per output frame so the
        // track's bars land exactly on master bars (pitch rides).
        let speed = g.interval / first.interval;

        // Equal-loudness: gain-match the section's source RMS to target.
        let src_start = g.phase + start_bar as f64 * g.interval * BEATS_PER_BAR;
        let src_frames = bars as f64 * g.interval * BEATS_PER_BAR;
        let rms = region_rms(&tracks[ti].samples, src_start as usize, src_frames as usize);
        let gain = if rms > 1e-6 {
            (TARGET_RMS / rms).min(MAX_GAIN)
        } else {
            1.0
        };

        // The boundary's transition: seeded to the reference proportions
        // (70% swap / 25% cut / 5% fade). The mix's opening section has
        // no boundary to mark, so it always swaps in.
        let transition = if pos == 0 {
            Transition::Swap
        } else {
            match rng.next() % 100 {
                0..=69 => Transition::Swap,
                70..=94 => Transition::Cut,
                _ => {
                    // A short, musical ramp: 2, 4, or 8 beats.
                    let beats = [2u8, 4, 8][(rng.next() % 3) as usize];
                    Transition::Fade { beats }
                }
            }
        };

        // Butt-join on the master grid: this section starts exactly where
        // the previous ended, which is a phrase boundary by construction.
        let out_start = (out_bars as f64 * master_fpb).round() as u64;
        let out_end = ((out_bars + bars) as f64 * master_fpb).round() as u64;
        sections.push(Section {
            track: ti,
            order_pos: pos,
            start_bar,
            bars,
            out_start,
            out_frames: out_end - out_start,
            speed,
            gain,
            transition,
        });
        out_bars += bars;
    }

    // ── fader chops: 0–2 per section, ~30–80 ms, un-quantized offsets
    // inside the section (never in its first or last bar, so transitions
    // stay clean). Macro on the grid, micro human.
    let mut chops = Vec::new();
    for s in &sections {
        if s.bars < 3 {
            continue;
        }
        let n = rng.next() % 3;
        for _ in 0..n {
            let span = s.out_frames as f64 - 2.0 * master_fpb;
            let off = master_fpb + (rng.next() as f64 / u64::MAX as f64) * span;
            let ms = 30 + rng.next() % 51; // 30–80 ms
            let frames = (ms as f64 * RENDER_RATE as f64 / 1000.0) as u64;
            chops.push(Chop {
                out_start: s.out_start + off as u64,
                frames,
            });
        }
    }
    chops.sort_by_key(|c| c.out_start);

    // ── scratch flurries: a handful per hour, clustered into one stretch
    // of the mix; each starts on a beat (leaning beat 2) and rubs an
    // onset-rich half-beat slice of the track playing underneath.
    let fpbeat = master_fpb / BEATS_PER_BAR;
    let total_bars: u64 = sections.iter().map(|s| s.bars).sum();
    let total_frames = (out_bars as f64 * master_fpb) as u64;
    let n_flurries = (total_bars / 128).max(1) as usize;
    // The cluster: a window of ~30% of the mix, seeded into its middle.
    let win_w = (total_bars as f64 * 0.30).max(8.0);
    let win_lo =
        1.0 + (rng.next() as f64 / u64::MAX as f64) * (total_bars as f64 - win_w - 2.0).max(0.0);
    let mut flurries: Vec<Flurry> = Vec::with_capacity(n_flurries);
    for _ in 0..n_flurries {
        // Bar inside the cluster window + a beat offset leaning on 2.
        let bar = win_lo + (rng.next() as f64 / u64::MAX as f64) * win_w;
        let beat = match rng.next() % 6 {
            0..=2 => 1u64, // beat 2 (index 1) — the reference's lean
            3 => 0,
            4 => 2,
            _ => 3,
        };
        let out_start = ((bar.floor() * BEATS_PER_BAR + beat as f64) * fpbeat).round() as u64;
        let frames = (RENDER_RATE as f64 * (1.0 + (rng.next() as f64 / u64::MAX as f64))) as u64;
        if out_start + frames >= total_frames {
            continue;
        }
        // The section (and so the track) under the flurry.
        let Some(sec) = sections
            .iter()
            .find(|s| out_start >= s.out_start && out_start < s.out_start + s.out_frames)
        else {
            continue;
        };
        let g = &grids[sec.track];
        let slice_frames = (g.interval / 2.0) as u64; // half a beat of source
        let src_lo = g.phase + sec.start_bar as f64 * g.interval * BEATS_PER_BAR;
        let src_span = sec.bars as f64 * g.interval * BEATS_PER_BAR - slice_frames as f64;
        // Onset-rich ≈ loudest of 8 seeded candidate slices.
        let mut best = (0.0f32, src_lo);
        for _ in 0..8 {
            let cand = src_lo + (rng.next() as f64 / u64::MAX as f64) * src_span.max(1.0);
            let r = region_rms(
                &tracks[sec.track].samples,
                cand as usize,
                slice_frames as usize,
            );
            if r > best.0 {
                best = (r, cand);
            }
        }
        let (slice_rms, slice_start) = best;
        let gain = if slice_rms > 1e-6 {
            (0.9 * TARGET_RMS / slice_rms).min(MAX_GAIN)
        } else {
            1.0
        };
        // Strokes: alternating whips and wikis, durations jittered ±35%
        // around a base rub — never snapped to a 16th.
        let mut strokes = Vec::new();
        let base = fpbeat / 3.0; // a brisk rub half-stroke
        let mut tpos = 0.0f64;
        let mut whip_turn = rng.next() % 2 == 0;
        while tpos < frames as f64 {
            let jit = |rng: &mut SplitMix64| 0.65 + 0.7 * (rng.next() as f64 / u64::MAX as f64);
            let fwd = base * jit(&mut rng);
            let back = base * jit(&mut rng);
            let (sa, sb) = (slice_start, slice_start + slice_frames as f64);
            strokes.push(ScratchStroke {
                offset: tpos,
                dur: fwd,
                from: sa,
                to: sb,
                gain: if whip_turn { 0.0 } else { 1.0 }, // whip: push muted
            });
            strokes.push(ScratchStroke {
                offset: tpos + fwd,
                dur: back,
                from: sb,
                to: sa,
                gain: 1.0, // the pull-back always sounds
            });
            tpos += fwd + back;
            whip_turn = !whip_turn;
        }
        flurries.push(Flurry {
            out_start,
            frames,
            track: sec.track,
            gain,
            strokes,
        });
    }
    flurries.sort_by_key(|fl| fl.out_start);

    Ok(MixPlan {
        master_bpm,
        frames_per_bar: master_fpb,
        seed,
        sections,
        flurries,
        chops,
    })
}

/// Render a plan to one interleaved stereo buffer at [`RENDER_RATE`].
/// Varispeed is linear-interpolated (pitch rides speed, platter-style).
pub fn render(plan: &MixPlan, tracks: &[TrackInput]) -> Vec<f32> {
    let total = plan.total_frames() as usize;
    let mut out = vec![0.0f32; total * 2];
    for s in &plan.sections {
        let tr = &tracks[s.track];
        let grid_phase = fit_grid(&tr.beats).map(|(p, _)| p).unwrap_or(0.0);
        let interval = fit_grid(&tr.beats).map(|(_, i)| i).unwrap_or(1.0);
        let src_start = grid_phase + s.start_bar as f64 * interval * BEATS_PER_BAR;
        let frames_in = tr.samples.len() / 2;
        let fpbeat = plan.frames_per_bar / BEATS_PER_BAR; // master beat, frames
        for k in 0..s.out_frames as usize {
            let src = src_start + k as f64 * s.speed;
            let i = src as usize;
            if i + 1 >= frames_in {
                break; // ran off the end (rounding); leave silence
            }
            let frac = (src - i as f64) as f32;
            // The entry envelope: how the transition shapes this section's
            // first moments (it is unity after the entry settles).
            let g = s.gain * entry_envelope(s.transition, k as f64, fpbeat);
            let oi = (s.out_start as usize + k) * 2;
            for c in 0..2 {
                let a = tr.samples[i * 2 + c];
                let b = tr.samples[(i + 1) * 2 + c];
                out[oi + c] = (a + (b - a) * frac) * g;
            }
        }
    }
    // The performance layer: fader chops cut the bed; scratch flurries
    // overlay it. Both are plan-level, so both are deterministic.
    apply_chops(plan, &mut out);
    apply_flurries(plan, tracks, &mut out);
    out
}

/// RMS of an interleaved-stereo region (both channels pooled).
fn region_rms(samples: &[f32], start_frame: usize, frames: usize) -> f32 {
    let total = samples.len() / 2;
    let end = (start_frame + frames).min(total);
    if end <= start_frame {
        return 0.0;
    }
    let slice = &samples[start_frame * 2..end * 2];
    let sum: f64 = slice.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / slice.len() as f64).sqrt() as f32
}

/// Apply the plan's fader chops: each cuts the bed to silence for its
/// window, with 1 ms edge ramps so the cut itself doesn't click.
fn apply_chops(plan: &MixPlan, out: &mut [f32]) {
    let ramp = (RENDER_RATE as f64 / 1000.0) as u64; // 1 ms
    let total = (out.len() / 2) as u64;
    for c in &plan.chops {
        let end = (c.out_start + c.frames).min(total);
        for fr in c.out_start..end {
            let into = fr - c.out_start;
            let left = end - fr;
            let g = if into < ramp {
                1.0 - into as f32 / ramp as f32
            } else if left <= ramp {
                1.0 - left as f32 / ramp as f32
            } else {
                0.0
            };
            out[(fr * 2) as usize] *= g;
            out[(fr * 2 + 1) as usize] *= g;
        }
    }
}

/// Overlay the plan's scratch flurries: each stroke reads its slice
/// linearly (forward or back — pitch rides the hand speed), gated by the
/// whip/wiki fader gain, with 3 ms edge fades against clicks.
fn apply_flurries(plan: &MixPlan, tracks: &[TrackInput], out: &mut [f32]) {
    let edge = RENDER_RATE as f64 * 0.003; // 3 ms de-click
    let total = (out.len() / 2) as u64;
    for fl in &plan.flurries {
        let tr = &tracks[fl.track];
        let frames_in = tr.samples.len() / 2;
        for s in &fl.strokes {
            if s.gain == 0.0 || s.dur < 1.0 {
                continue; // a muted push moves the hand, not the speaker
            }
            let n = s.dur as usize;
            for k in 0..n {
                let ofr = fl.out_start + s.offset as u64 + k as u64;
                if ofr >= total || (s.offset + k as f64) >= fl.frames as f64 {
                    break;
                }
                let pos = s.from + (s.to - s.from) * (k as f64 / s.dur);
                let i = pos as usize;
                if i + 1 >= frames_in {
                    continue;
                }
                let frac = (pos - i as f64) as f32;
                // edge fades within the stroke
                let e_in = (k as f64 / edge).min(1.0);
                let e_out = ((n - k) as f64 / edge).min(1.0);
                let g = fl.gain * s.gain * (e_in.min(e_out)) as f32;
                let oi = (ofr * 2) as usize;
                for c in 0..2 {
                    let a = tr.samples[i * 2 + c];
                    let b = tr.samples[(i + 1) * 2 + c];
                    out[oi + c] += (a + (b - a) * frac) * g;
                }
            }
        }
    }
}

/// The multiplier a transition applies at `k` output frames into its
/// section (`fpbeat` = master frames per beat). Unity once settled.
fn entry_envelope(tr: Transition, k: f64, fpbeat: f64) -> f32 {
    match tr {
        Transition::Swap => 1.0,
        Transition::Cut => {
            // +6 dB punch for the first beat, easing back to unity over
            // the second — punctuation, not a level error.
            const PUNCH: f64 = 2.0;
            if k < fpbeat {
                PUNCH as f32
            } else if k < 2.0 * fpbeat {
                (PUNCH - (PUNCH - 1.0) * (k - fpbeat) / fpbeat) as f32
            } else {
                1.0
            }
        }
        Transition::Fade { beats } => {
            let ramp = beats as f64 * fpbeat;
            if k < ramp {
                (k / ramp) as f32
            } else {
                1.0
            }
        }
    }
}

/// FNV-1a 64 — tiny, dependency-free, platform-stable input hashing.
struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Fnv64(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

/// SplitMix64 — a tiny, well-distributed, platform-stable PRNG. All the
/// engine's "taste" flows from this, seeded by the input, so the same
/// sequence always makes the same choices.
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic track: `secs` of a sine at `hz`, RMS ≈ amp/√2, with
    /// beat marks every `interval` frames starting at `phase`.
    fn tone_track(id: &str, secs: f64, hz: f64, amp: f32, phase: u64, interval: u64) -> TrackInput {
        let frames = (secs * RENDER_RATE as f64) as usize;
        let mut samples = Vec::with_capacity(frames * 2);
        for n in 0..frames {
            let v = amp
                * (2.0 * std::f64::consts::PI * hz * n as f64 / RENDER_RATE as f64).sin() as f32;
            samples.push(v);
            samples.push(v);
        }
        let n_beats = ((frames as u64).saturating_sub(phase)) / interval;
        let beats: Vec<u64> = (0..n_beats).map(|k| phase + k * interval).collect();
        TrackInput {
            id: id.to_string(),
            samples: Arc::new(samples),
            beats,
        }
    }

    /// 120 BPM at 44100 Hz: 22050 frames per beat.
    const FPB_120: u64 = 22_050;

    fn two_long_tracks() -> Vec<TrackInput> {
        vec![
            // 120 BPM, 80 s ≈ 40 bars available.
            tone_track("a", 80.0, 220.0, 0.5, 1000, FPB_120),
            // 90 BPM (29400 frames/beat), 90 s ≈ 17 bars.
            tone_track("b", 90.0, 330.0, 0.1, 500, 29_400),
        ]
    }

    #[test]
    fn first_track_sets_master_and_speed_ratios() {
        let tracks = two_long_tracks();
        let p = plan(&tracks, &[0, 1]).unwrap();
        assert!((p.master_bpm - 120.0).abs() < 0.01, "bpm {}", p.master_bpm);
        assert!((p.sections[0].speed - 1.0).abs() < 1e-9);
        // 90 BPM source under a 120 BPM master: consume 29400/22050 = 4/3
        // source frames per output frame (faster, pitch up — varispeed).
        assert!(
            (p.sections[1].speed - 4.0 / 3.0).abs() < 1e-6,
            "speed {}",
            p.sections[1].speed
        );
    }

    #[test]
    fn sections_are_phrase_sized_and_butt_joined() {
        let tracks = two_long_tracks();
        let p = plan(&tracks, &[0, 1, 0]).unwrap();
        let fpb = p.frames_per_bar;
        let mut expect_start = 0u64;
        let mut bars_cum = 0u64;
        for s in &p.sections {
            assert!((8..=16).contains(&s.bars), "bars {}", s.bars);
            assert_eq!(s.out_start, expect_start, "butt joint broken");
            // Every boundary lands on a master bar line.
            bars_cum += s.bars;
            let want_end = (bars_cum as f64 * fpb).round() as u64;
            assert_eq!(s.out_start + s.out_frames, want_end);
            expect_start = want_end;
        }
        assert_eq!(p.total_frames(), expect_start);
    }

    #[test]
    fn repeat_entries_pick_different_material() {
        let tracks = two_long_tracks();
        let p = plan(&tracks, &[0, 1, 0, 0]).unwrap();
        let track0_starts: Vec<u64> = p
            .sections
            .iter()
            .filter(|s| s.track == 0)
            .map(|s| s.start_bar)
            .collect();
        assert_eq!(track0_starts.len(), 3);
        assert!(
            track0_starts[0] != track0_starts[1] && track0_starts[1] != track0_starts[2],
            "repeats reused material: {track0_starts:?}"
        );
    }

    #[test]
    fn sections_render_loudness_matched() {
        let tracks = two_long_tracks(); // amps 0.5 and 0.1 — very different
        let p = plan(&tracks, &[0, 1]).unwrap();
        let mix = render(&p, &tracks);
        for s in &p.sections {
            // Skip the joint frames; measure the section's interior.
            let a = (s.out_start + 1000) as usize * 2;
            let b = (s.out_start + s.out_frames - 1000) as usize * 2;
            let sum: f64 = mix[a..b].iter().map(|&x| (x as f64) * (x as f64)).sum();
            let rms = (sum / (b - a) as f64).sqrt() as f32;
            assert!(
                (rms - TARGET_RMS).abs() < 0.02,
                "section at {} has RMS {} (want ≈{})",
                s.out_start,
                rms,
                TARGET_RMS
            );
        }
    }

    #[test]
    fn same_input_same_plan_and_bytes() {
        let tracks = two_long_tracks();
        let order = [0usize, 1, 0];
        let p1 = plan(&tracks, &order).unwrap();
        let p2 = plan(&tracks, &order).unwrap();
        assert_eq!(p1, p2, "plan must be deterministic");
        let m1 = render(&p1, &tracks);
        let m2 = render(&p2, &tracks);
        assert_eq!(m1, m2, "render must be bit-identical");
    }

    #[test]
    fn different_order_different_seed() {
        let tracks = two_long_tracks();
        let p1 = plan(&tracks, &[0, 1]).unwrap();
        let p2 = plan(&tracks, &[1, 0]).unwrap();
        assert_ne!(p1.seed, p2.seed, "order must feed the seed");
    }

    #[test]
    fn short_track_clamps_to_available_bars() {
        // ~6 bars of 120 BPM: a section must clamp, not fail.
        let tracks = vec![tone_track("short", 12.5, 220.0, 0.3, 0, FPB_120)];
        let p = plan(&tracks, &[0]).unwrap();
        assert!(p.sections[0].bars <= 6, "bars {}", p.sections[0].bars);
        assert!(p.sections[0].bars >= 1);
    }

    #[test]
    fn transition_distribution_matches_the_reference() {
        // Two long tracks, many boundaries: ~70/25/5 within tolerance and
        // identical on every run (the schedule is the seed's).
        let tracks = two_long_tracks();
        let order: Vec<usize> = (0..400).map(|k| k % 2).collect();
        let p1 = plan(&tracks, &order).unwrap();
        let p2 = plan(&tracks, &order).unwrap();
        assert_eq!(p1, p2, "schedule must be deterministic");

        assert_eq!(
            p1.sections[0].transition,
            Transition::Swap,
            "opening always swaps"
        );
        let n = (p1.sections.len() - 1) as f64;
        let count = |f: &dyn Fn(Transition) -> bool| {
            p1.sections[1..].iter().filter(|s| f(s.transition)).count() as f64 / n
        };
        let swaps = count(&|t| t == Transition::Swap);
        let cuts = count(&|t| t == Transition::Cut);
        let fades = count(&|t| matches!(t, Transition::Fade { .. }));
        assert!((swaps - 0.70).abs() < 0.07, "swaps {swaps}");
        assert!((cuts - 0.25).abs() < 0.07, "cuts {cuts}");
        assert!((fades - 0.05).abs() < 0.04, "fades {fades}");
        // Fade lengths are musical: 2, 4, or 8 beats.
        for s in &p1.sections {
            if let Transition::Fade { beats } = s.transition {
                assert!(matches!(beats, 2 | 4 | 8), "fade beats {beats}");
            }
        }
        // Transitions never move a boundary: still butt-joined on bars.
        let mut expect = 0u64;
        for s in &p1.sections {
            assert_eq!(s.out_start, expect);
            expect = s.out_start + s.out_frames;
        }
    }

    /// RMS of one channel-interleaved window of the mix, in frames.
    fn win_rms(mix: &[f32], start_frame: usize, frames: usize) -> f32 {
        let a = start_frame * 2;
        let b = ((start_frame + frames) * 2).min(mix.len());
        let sum: f64 = mix[a..b].iter().map(|&x| (x as f64) * (x as f64)).sum();
        (sum / (b - a).max(1) as f64).sqrt() as f32
    }

    #[test]
    fn cut_punches_and_fade_ramps_in_the_render() {
        // Enough boundaries that the seed deals at least one cut and one
        // fade (the schedule is deterministic, so this can't flake).
        let tracks = two_long_tracks();
        let order: Vec<usize> = (0..24).map(|k| k % 2).collect();
        let p = plan(&tracks, &order).unwrap();
        let mix = render(&p, &tracks);
        let fpbeat = (p.frames_per_bar / BEATS_PER_BAR) as usize;

        let cut = p.sections.iter().find(|s| s.transition == Transition::Cut);
        let fade = p
            .sections
            .iter()
            .find(|s| matches!(s.transition, Transition::Fade { .. }));
        let (cut, fade) = (cut.expect("a cut in 39 boundaries"), fade.expect("a fade"));

        // The cut's first beat sits ≈ +6 dB over its settled interior.
        let punch = win_rms(&mix, cut.out_start as usize, fpbeat);
        let settled = win_rms(&mix, cut.out_start as usize + 4 * fpbeat, 4 * fpbeat);
        let ratio = punch / settled.max(1e-6);
        assert!((1.6..=2.4).contains(&ratio), "punch ratio {ratio}");

        // The fade climbs monotonically from near-silence to its level.
        let Transition::Fade { beats } = fade.transition else {
            unreachable!()
        };
        let ramp = beats as usize * fpbeat;
        let q = ramp / 4;
        let mut last = 0.0f32;
        for w in 0..4 {
            let r = win_rms(&mix, fade.out_start as usize + w * q, q);
            assert!(r >= last * 0.9, "fade window {w} fell: {r} < {last}");
            last = r;
        }
        let head = win_rms(&mix, fade.out_start as usize, q);
        let tail = win_rms(&mix, fade.out_start as usize + ramp, fpbeat);
        assert!(
            head < tail * 0.6,
            "fade head {head} not quiet vs tail {tail}"
        );
    }

    #[test]
    fn flurries_start_on_beats_leaning_two_and_cluster() {
        let tracks = two_long_tracks();
        let order: Vec<usize> = (0..200).map(|k| k % 2).collect();
        let p = plan(&tracks, &order).unwrap();
        assert!(p.flurries.len() >= 3, "flurries {}", p.flurries.len());

        let fpbeat = p.frames_per_bar / BEATS_PER_BAR;
        let mut beat_counts = [0usize; 4];
        for fl in &p.flurries {
            // Starts ON the beat grid (within rounding).
            let beats = fl.out_start as f64 / fpbeat;
            assert!(
                (beats - beats.round()).abs() * fpbeat < 2.0,
                "flurry off-grid at {}",
                fl.out_start
            );
            beat_counts[(beats.round() as u64 % 4) as usize] += 1;
        }
        // The lean: beat 2 (index 1) is the most common start.
        let max_other = [beat_counts[0], beat_counts[2], beat_counts[3]]
            .into_iter()
            .max()
            .unwrap();
        assert!(
            beat_counts[1] >= max_other,
            "no beat-2 lean: {beat_counts:?}"
        );

        // Clustered: every flurry inside ~a third of the mix, not uniform.
        let lo = p.flurries.iter().map(|f| f.out_start).min().unwrap();
        let hi = p.flurries.iter().map(|f| f.out_start).max().unwrap();
        assert!(
            (hi - lo) as f64 <= 0.35 * p.total_frames() as f64,
            "flurries not clustered: span {} of {}",
            hi - lo,
            p.total_frames()
        );
    }

    #[test]
    fn micro_timing_is_not_sixteenth_quantized() {
        let tracks = two_long_tracks();
        let order: Vec<usize> = (0..200).map(|k| k % 2).collect();
        let p = plan(&tracks, &order).unwrap();
        let sixteenth = p.frames_per_bar / 16.0;

        // Stroke onsets: distance to the nearest 16th ≈ chance (0.25),
        // nowhere near quantized (~0).
        let mut dists = Vec::new();
        for fl in &p.flurries {
            for s in &fl.strokes {
                let abs = fl.out_start as f64 + s.offset;
                let q = (abs / sixteenth).round() * sixteenth;
                dists.push(((abs - q).abs() / sixteenth).min(0.5));
            }
        }
        assert!(dists.len() > 40, "strokes {}", dists.len());
        let mean = dists.iter().sum::<f64>() / dists.len() as f64;
        assert!(mean > 0.12, "stroke onsets look quantized: mean {mean}");

        // Chop offsets too.
        let mut cd = Vec::new();
        for c in &p.chops {
            let q = (c.out_start as f64 / sixteenth).round() * sixteenth;
            cd.push(((c.out_start as f64 - q).abs() / sixteenth).min(0.5));
        }
        assert!(cd.len() > 30, "chops {}", cd.len());
        let cmean = cd.iter().sum::<f64>() / cd.len() as f64;
        assert!(cmean > 0.12, "chop offsets look quantized: mean {cmean}");

        // And chops are fader-cut sized: 30–80 ms.
        for c in &p.chops {
            let ms = c.frames as f64 / RENDER_RATE as f64 * 1000.0;
            assert!((29.0..=81.0).contains(&ms), "chop {ms} ms");
        }
    }

    #[test]
    fn chops_cut_and_flurries_add_energy_in_the_render() {
        let tracks = two_long_tracks();
        let order: Vec<usize> = (0..10).map(|k| k % 2).collect();
        let p = plan(&tracks, &order).unwrap();
        let mix = render(&p, &tracks);

        // A chop window is near-silent against its surroundings.
        let c = p.chops.first().expect("a chop in 10 sections");
        let inside = win_rms(&mix, c.out_start as usize + 50, c.frames as usize - 100);
        let before = win_rms(&mix, c.out_start as usize - 4000, 3000);
        assert!(
            inside < before * 0.15,
            "chop not cutting: inside {inside} vs before {before}"
        );

        // Flurries add energy: the same plan stripped of them is quieter
        // across the flurry window (everything else identical).
        if let Some(fl) = p.flurries.first() {
            let mut stripped = p.clone();
            stripped.flurries.clear();
            let bed = render(&stripped, &tracks);
            let with = win_rms(&mix, fl.out_start as usize, fl.frames as usize);
            let without = win_rms(&bed, fl.out_start as usize, fl.frames as usize);
            assert!(
                with > without * 1.05,
                "flurry inaudible: {with} vs bed {without}"
            );
        } else {
            panic!("no flurry in a 10-section mix");
        }
    }

    #[test]
    fn plan_errors_are_specific() {
        assert_eq!(plan(&[], &[]), Err(PlanError::EmptySequence));
        let one = vec![tone_track("a", 30.0, 220.0, 0.3, 0, FPB_120)];
        assert_eq!(plan(&one, &[3]), Err(PlanError::BadOrderIndex(0)));
        let mut untapped = vec![tone_track("a", 30.0, 220.0, 0.3, 0, FPB_120)];
        untapped[0].beats.truncate(1);
        assert_eq!(plan(&untapped, &[0]), Err(PlanError::NotEnoughBeats(0)));
    }
}
