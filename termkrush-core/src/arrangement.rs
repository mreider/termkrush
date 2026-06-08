//! The free-track arrangement: the DAW-style timeline model (2026-06-08 GUI
//! pivot). Unlike the retired step-grid [`timeline`](crate::timeline), tracks
//! are not bound to pads — each track holds **blocks**, a placed clip (its own
//! samples) at an arbitrary start frame. You drag clips onto tracks, move them,
//! copy/paste them, and render the whole thing to one buffer.
//!
//! This is the headless model; the GUI draws + edits it, and playback layers on
//! in their own stories. Samples are interleaved stereo `f32` at the mix rate.

use std::sync::Arc;

/// A placed clip on a track: its samples and where it starts on the timeline.
#[derive(Debug, Clone)]
pub struct Block {
    /// Interleaved stereo samples (the clip dropped here).
    pub samples: Arc<Vec<f32>>,
    /// Start position on the timeline, in frames.
    pub start: u64,
    /// Display label (the source track name).
    pub label: String,
}

impl Block {
    /// Length in stereo frames.
    pub fn len_frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// One past the last frame this block occupies on the timeline.
    pub fn end(&self) -> u64 {
        self.start + self.len_frames() as u64
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
}

impl Arrangement {
    /// A new arrangement with `tracks` empty lanes.
    pub fn new(sample_rate: u32, tracks: usize) -> Self {
        Self {
            tracks: vec![Track::default(); tracks.max(1)],
            sample_rate: sample_rate.max(1),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
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

    /// Remove block `idx` from `track`, returning it (e.g. to copy/paste).
    pub fn remove_block(&mut self, track: usize, idx: usize) -> Option<Block> {
        let t = self.tracks.get_mut(track)?;
        if idx < t.blocks.len() {
            Some(t.blocks.remove(idx))
        } else {
            None
        }
    }

    /// Total length in frames — one past the last block end, or 0 if empty.
    pub fn total_frames(&self) -> u64 {
        self.tracks
            .iter()
            .flat_map(|t| t.blocks.iter())
            .map(|b| b.end())
            .max()
            .unwrap_or(0)
    }

    /// Sum the arrangement into `out` (interleaved stereo) starting at frame
    /// `playhead` — for live transport playback. `out.len()/2` frames are mixed;
    /// blocks overlapping the window contribute their samples.
    pub fn mix_into(&self, playhead: u64, out: &mut [f32]) {
        let frames = out.len() / 2;
        for track in &self.tracks {
            for block in &track.blocks {
                let (bs, be) = (block.start, block.end());
                // Window of global frames covered by this call: [playhead, end).
                let win_end = playhead + frames as u64;
                if be <= playhead || bs >= win_end {
                    continue; // block not in this window
                }
                let from = bs.max(playhead);
                let to = be.min(win_end);
                for gf in from..to {
                    let oi = (gf - playhead) as usize * 2;
                    let bi = (gf - bs) as usize * 2;
                    out[oi] += block.samples[bi];
                    out[oi + 1] += block.samples[bi + 1];
                }
            }
        }
    }

    /// Render the whole arrangement to one interleaved-stereo buffer, summing
    /// every block at its start position. Empty if there are no blocks.
    pub fn render(&self) -> Vec<f32> {
        let total = self.total_frames() as usize;
        if total == 0 {
            return Vec::new();
        }
        let mut out = vec![0.0f32; total * 2];
        for track in &self.tracks {
            for block in &track.blocks {
                let base = block.start as usize * 2;
                for (k, s) in block.samples.iter().enumerate() {
                    out[base + k] += s;
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
        }
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
    fn empty_arrangement_renders_nothing() {
        let a = Arrangement::new(44_100, 4);
        assert_eq!(a.total_frames(), 0);
        assert!(a.render().is_empty());
    }
}
