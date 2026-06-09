//! The free-track arrangement: the DAW-style timeline model (2026-06-08 GUI
//! pivot). Unlike the retired step-grid tracker it replaced, tracks
//! are not bound to pads — each track holds **blocks**, a placed clip (its own
//! samples) at an arbitrary start frame. You drag clips onto tracks, move them,
//! copy/paste them, and render the whole thing to one buffer.
//!
//! This is the headless model; the GUI draws + edits it, and playback layers on
//! in their own stories. Samples are interleaved stereo `f32` at the mix rate.

use std::sync::Arc;

/// Where a block's first onset (its musical hit) is aligned on the master grid.
/// Tempo is always synced; this is the *phase* — cycled by the per-block button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Onset on the nearest beat (the snap-on-drop default).
    #[default]
    OnBeat,
    /// Onset on the bar's downbeat (1 of 4).
    Bar,
    /// Onset on the off-beat (the "&", +½ beat).
    OffBeat,
    /// No onset correction — the clip's file start sits on the beat.
    Free,
}

/// A placed clip on a track: its samples and where it starts on the timeline.
#[derive(Debug, Clone)]
pub struct Block {
    /// Interleaved stereo samples (the clip dropped here).
    pub samples: Arc<Vec<f32>>,
    /// Start position on the timeline, in frames.
    pub start: u64,
    /// Display label (the source track name).
    pub label: String,
    /// The clip/pad this block was placed from, if any — so edits to that clip
    /// (trim / volume) can re-flow into the block. `None` for library drops.
    pub source_pad: Option<usize>,
    /// The source clip's detected tempo, if known (the MASTER track uses it).
    pub bpm: Option<f32>,
    /// First-onset offset in *source* frames (cached on drop), so the grid snap
    /// can land the hit, not the file head.
    pub onset: u64,
    /// How the onset is aligned to the grid.
    pub phase: Phase,
    /// Tempo-lock this block to the master (varispeed). Only loops want this; a
    /// one-shot is a single hit with no tempo, so it plays at its native pitch
    /// and is merely phase-placed (varispeeding it would chipmunk it).
    pub sync: bool,
}

impl Block {
    /// Length in stereo frames at the clip's native tempo.
    pub fn len_frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// Native end (one past the last source frame), ignoring varispeed.
    pub fn end(&self) -> u64 {
        self.start + self.len_frames() as u64
    }

    /// Source read speed to play this block at `target` tempo. Only `sync`
    /// (loop) blocks varispeed (a 120-BPM loop under a 240 master reads at 2×);
    /// one-shots and scratch play at native rate (1.0), so they never chipmunk.
    pub fn speed(&self, target: Option<f32>) -> f64 {
        if !self.sync {
            return 1.0;
        }
        match (self.bpm, target) {
            (Some(b), Some(t)) if b > 0.0 && t > 0.0 => (t / b) as f64,
            _ => 1.0,
        }
    }

    /// Output length in timeline frames when played at `target` (varispeed:
    /// faster → fewer output frames).
    pub fn out_frames(&self, target: Option<f32>) -> u64 {
        let s = self.speed(target);
        if s <= 0.0 {
            return self.len_frames() as u64;
        }
        (self.len_frames() as f64 / s).round() as u64
    }

    /// One past the last timeline frame this block occupies at `target`.
    pub fn end_at(&self, target: Option<f32>) -> u64 {
        self.start + self.out_frames(target)
    }

    /// Sample (linear-interpolated) at output offset `o` and channel `ch`,
    /// reading the source at the varispeed rate for `target`.
    fn sample_at(&self, target: Option<f32>, o: u64, ch: usize) -> f32 {
        let src = o as f64 * self.speed(target);
        let i = src.floor() as usize;
        let n = self.len_frames();
        if i >= n {
            return 0.0;
        }
        let a = self.samples[i * 2 + ch];
        let b = if i + 1 < n {
            self.samples[(i + 1) * 2 + ch]
        } else {
            a
        };
        let frac = (src - i as f64) as f32;
        a + (b - a) * frac
    }
}

/// One horizontal lane holding any number of blocks.
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub blocks: Vec<Block>,
}

/// The whole arrangement: a set of tracks at a fixed sample rate.
#[derive(Debug, Clone)]
pub struct Arrangement {
    tracks: Vec<Track>,
    sample_rate: u32,
    /// Master tempo every block varispeeds to (set by the MASTER track). `None`
    /// → blocks play at their native rate.
    target_bpm: Option<f32>,
}

impl Arrangement {
    /// A new arrangement with `tracks` empty lanes.
    pub fn new(sample_rate: u32, tracks: usize) -> Self {
        Self {
            tracks: vec![Track::default(); tracks.max(1)],
            sample_rate: sample_rate.max(1),
            target_bpm: None,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The master tempo all blocks lock to.
    pub fn target_bpm(&self) -> Option<f32> {
        self.target_bpm
    }

    /// Set the master tempo (every block varispeeds to it).
    pub fn set_target_bpm(&mut self, bpm: Option<f32>) {
        self.target_bpm = bpm;
    }

    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Append a new empty track; returns its index.
    pub fn add_track(&mut self) -> usize {
        self.tracks.push(Track::default());
        self.tracks.len() - 1
    }

    /// Place `block` on `track`. No-op (returns `None`) for a bad track index;
    /// otherwise returns the block's index within that track.
    pub fn add_block(&mut self, track: usize, block: Block) -> Option<usize> {
        let t = self.tracks.get_mut(track)?;
        t.blocks.push(block);
        Some(t.blocks.len() - 1)
    }

    /// Move block `idx` from `track` to `(new_track, new_start)`. Returns the
    /// new index in the destination track, or `None` if either index is bad.
    pub fn move_block(
        &mut self,
        track: usize,
        idx: usize,
        new_track: usize,
        new_start: u64,
    ) -> Option<usize> {
        if track >= self.tracks.len() || new_track >= self.tracks.len() {
            return None;
        }
        if idx >= self.tracks[track].blocks.len() {
            return None;
        }
        let mut block = self.tracks[track].blocks.remove(idx);
        block.start = new_start;
        let dst = &mut self.tracks[new_track].blocks;
        dst.push(block);
        Some(dst.len() - 1)
    }

    /// Replace the samples of every block sourced from `pad` — called after the
    /// clip on that pad is re-trimmed or its volume changes, so the placed blocks
    /// stay in sync with the clip.
    pub fn refresh_pad(&mut self, pad: usize, samples: Arc<Vec<f32>>) {
        for track in &mut self.tracks {
            for block in &mut track.blocks {
                if block.source_pad == Some(pad) {
                    block.samples = samples.clone();
                }
            }
        }
    }

    /// Sever the clip link on every block sourced from `pad` (keeping their
    /// samples) — used when the pad is cleared, so a later clip loaded there
    /// can't hijack these already-placed blocks.
    pub fn unlink_pad(&mut self, pad: usize) {
        for track in &mut self.tracks {
            for block in &mut track.blocks {
                if block.source_pad == Some(pad) {
                    block.source_pad = None;
                }
            }
        }
    }

    /// Mutable access to a block (e.g. to change its phase + start).
    pub fn block_mut(&mut self, track: usize, idx: usize) -> Option<&mut Block> {
        self.tracks.get_mut(track)?.blocks.get_mut(idx)
    }

    /// Remove block `idx` from `track`, returning it (e.g. to copy/paste).
    pub fn remove_block(&mut self, track: usize, idx: usize) -> Option<Block> {
        let t = self.tracks.get_mut(track)?;
        if idx < t.blocks.len() {
            Some(t.blocks.remove(idx))
        } else {
            None
        }
    }

    /// Total length in frames — one past the last (varispeed) block end, or 0.
    pub fn total_frames(&self) -> u64 {
        let target = self.target_bpm;
        self.tracks
            .iter()
            .flat_map(|t| t.blocks.iter())
            .map(|b| b.end_at(target))
            .max()
            .unwrap_or(0)
    }

    /// Sum the arrangement into `out` (interleaved stereo) starting at frame
    /// `playhead` — for live transport playback. Each block is varispeed-read to
    /// the master tempo, so different-tempo clips lock together.
    pub fn mix_into(&self, playhead: u64, out: &mut [f32]) {
        let frames = out.len() / 2;
        let win_end = playhead + frames as u64;
        let target = self.target_bpm;
        for track in &self.tracks {
            for block in &track.blocks {
                let (bs, be) = (block.start, block.end_at(target));
                if be <= playhead || bs >= win_end {
                    continue; // not in this window
                }
                for gf in bs.max(playhead)..be.min(win_end) {
                    let oi = (gf - playhead) as usize * 2;
                    let o = gf - bs; // output offset within the stretched block
                    out[oi] += block.sample_at(target, o, 0);
                    out[oi + 1] += block.sample_at(target, o, 1);
                }
            }
        }
    }

    /// Render the whole arrangement to one interleaved-stereo buffer, every block
    /// varispeed-locked to the master tempo. Empty if there are no blocks.
    pub fn render(&self) -> Vec<f32> {
        let total = self.total_frames() as usize;
        if total == 0 {
            return Vec::new();
        }
        let target = self.target_bpm;
        let mut out = vec![0.0f32; total * 2];
        for track in &self.tracks {
            for block in &track.blocks {
                let n = block.out_frames(target);
                for o in 0..n {
                    let base = (block.start + o) as usize * 2;
                    out[base] += block.sample_at(target, o, 0);
                    out[base + 1] += block.sample_at(target, o, 1);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(start: u64, frames: usize, fill: f32) -> Block {
        Block {
            samples: Arc::new(vec![fill; frames * 2]),
            start,
            label: "x".into(),
            source_pad: None,
            bpm: None,
            onset: 0,
            phase: Phase::default(),
            sync: false,
        }
    }

    #[test]
    fn refresh_pad_reflows_sourced_blocks_only() {
        let mut a = Arrangement::new(1000, 2);
        let mut b = block(0, 4, 0.2);
        b.source_pad = Some(3);
        a.add_block(0, b);
        a.add_block(1, block(0, 4, 0.2)); // source_pad None — untouched

        a.refresh_pad(3, Arc::new(vec![0.9; 8]));
        assert_eq!(
            a.tracks()[0].blocks[0].samples[0],
            0.9,
            "sourced block reflowed"
        );
        assert_eq!(
            a.tracks()[1].blocks[0].samples[0],
            0.2,
            "library block untouched"
        );
    }

    #[test]
    fn add_move_remove_and_total_length() {
        let mut a = Arrangement::new(44_100, 2);
        assert_eq!(a.track_count(), 2);
        a.add_block(0, block(100, 50, 0.5)); // occupies [100, 150)
        a.add_block(1, block(200, 30, 0.5)); // occupies [200, 230)
        assert_eq!(a.total_frames(), 230);

        // Move the first block onto track 1 starting at 300.
        let idx = a.move_block(0, 0, 1, 300).unwrap();
        assert!(a.tracks()[0].blocks.is_empty());
        assert_eq!(a.tracks()[1].blocks[idx].start, 300);
        assert_eq!(a.total_frames(), 350);

        // Bad indices are no-ops.
        assert!(a.move_block(9, 0, 0, 0).is_none());
        assert!(a.remove_block(0, 5).is_none());
    }

    #[test]
    fn render_places_blocks_at_their_start() {
        let mut a = Arrangement::new(1000, 1);
        a.add_block(0, block(10, 5, 0.5)); // frames 10..15
        let out = a.render();
        // total length = 15 frames -> 30 samples.
        assert_eq!(out.len(), 30);
        // Silence before the block, signal inside it.
        assert_eq!(out[0], 0.0);
        assert_eq!(out[9 * 2], 0.0, "frame 9 is before the block");
        assert_eq!(out[10 * 2], 0.5, "frame 10 is the block start");
        assert_eq!(out[14 * 2 + 1], 0.5, "last block frame, right channel");
    }

    #[test]
    fn overlapping_blocks_sum() {
        let mut a = Arrangement::new(1000, 2);
        a.add_block(0, block(0, 10, 0.4));
        a.add_block(1, block(5, 10, 0.4)); // overlaps frames 5..10
        let out = a.render();
        assert!((out[0] - 0.4).abs() < 1e-6, "only block A at frame 0");
        assert!(
            (out[6 * 2] - 0.8).abs() < 1e-6,
            "both blocks sum at frame 6"
        );
    }

    #[test]
    fn mix_into_places_blocks_at_the_playhead() {
        let mut a = Arrangement::new(1000, 1);
        a.add_block(0, block(10, 10, 0.5)); // frames 10..20
                                            // Window [5, 25): the block sounds at output offsets 5..15.
        let mut out = vec![0.0f32; 20 * 2];
        a.mix_into(5, &mut out);
        assert_eq!(out[4 * 2], 0.0, "before the block (global frame 9)");
        assert_eq!(out[5 * 2], 0.5, "block start (global frame 10) at offset 5");
        assert_eq!(out[14 * 2 + 1], 0.5, "last block frame, right channel");
        assert_eq!(out[15 * 2], 0.0, "after the block (global frame 20)");
    }

    #[test]
    fn varispeed_locks_blocks_to_the_master_tempo() {
        let mut a = Arrangement::new(1000, 1);
        a.set_target_bpm(Some(240.0));
        let mut b = block(0, 8, 0.5); // 8 native frames @ 120 BPM
        b.bpm = Some(120.0); // half the master → reads 2× → 4 output frames
        b.sync = true; // a loop: tempo-locks
        a.add_block(0, b);
        let blk = &a.tracks()[0].blocks[0];
        assert!((blk.speed(Some(240.0)) - 2.0).abs() < 1e-6);
        assert_eq!(blk.out_frames(Some(240.0)), 4, "plays in half the time");
        assert_eq!(a.total_frames(), 4);
        // With no target, it plays natively (8 frames).
        a.set_target_bpm(None);
        assert_eq!(a.total_frames(), 8);
    }

    #[test]
    fn one_shots_never_varispeed() {
        let mut a = Arrangement::new(1000, 1);
        a.set_target_bpm(Some(240.0));
        let mut b = block(0, 8, 0.5);
        b.bpm = Some(120.0); // would be 2× IF it synced…
        b.sync = false; // …but a one-shot plays native
        a.add_block(0, b);
        let blk = &a.tracks()[0].blocks[0];
        assert!((blk.speed(Some(240.0)) - 1.0).abs() < 1e-6, "no varispeed");
        assert_eq!(blk.out_frames(Some(240.0)), 8, "native length, no chipmunk");
    }

    #[test]
    fn empty_arrangement_renders_nothing() {
        let a = Arrangement::new(44_100, 4);
        assert_eq!(a.total_frames(), 0);
        assert!(a.render().is_empty());
    }
}
