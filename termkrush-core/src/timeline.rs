//! The arrangement: a tempo-locked tracker **step grid**.
//!
//! One lane per pad; columns are steps (subdivisions of a bar). A hit on a
//! step means "fire this pad there". Everything is quantized to the grid, so
//! the arrangement is always in time. Loop regions and playback layer on in
//! their own stories; this is the headless model.

use crate::mix::PADS;

/// A step-grid arrangement: `PADS` lanes × `bars * steps_per_bar` steps.
#[derive(Debug, Clone)]
pub struct Timeline {
    bars: usize,
    steps_per_bar: usize,
    /// `hits[lane][step]` — whether that pad fires on that step.
    hits: Vec<Vec<bool>>,
}

impl Timeline {
    /// A blank arrangement of `bars` bars at `steps_per_bar` steps each.
    pub fn new(bars: usize, steps_per_bar: usize) -> Self {
        let bars = bars.max(1);
        let steps_per_bar = steps_per_bar.max(1);
        let total = bars * steps_per_bar;
        Timeline {
            bars,
            steps_per_bar,
            hits: vec![vec![false; total]; PADS],
        }
    }

    pub fn bars(&self) -> usize {
        self.bars
    }

    pub fn steps_per_bar(&self) -> usize {
        self.steps_per_bar
    }

    /// Total number of steps across the whole arrangement.
    pub fn total_steps(&self) -> usize {
        self.bars * self.steps_per_bar
    }

    /// Whether pad `lane` fires on `step`.
    pub fn step(&self, lane: usize, step: usize) -> bool {
        self.hits
            .get(lane)
            .and_then(|l| l.get(step))
            .copied()
            .unwrap_or(false)
    }

    /// Set pad `lane`'s hit on `step`.
    pub fn set_step(&mut self, lane: usize, step: usize, on: bool) {
        if let Some(l) = self.hits.get_mut(lane) {
            if let Some(cell) = l.get_mut(step) {
                *cell = on;
            }
        }
    }

    /// Toggle pad `lane`'s hit on `step`; returns the new state.
    pub fn toggle(&mut self, lane: usize, step: usize) -> bool {
        let now = !self.step(lane, step);
        self.set_step(lane, step, now);
        now
    }

    /// Which pads fire on `step`, in lane order.
    pub fn pads_at(&self, step: usize) -> Vec<usize> {
        (0..PADS).filter(|&l| self.step(l, step)).collect()
    }

    /// Clear all hits.
    pub fn clear(&mut self) {
        for l in &mut self.hits {
            l.iter_mut().for_each(|c| *c = false);
        }
    }
}

impl Default for Timeline {
    /// Four bars of sixteenth-note steps — a comfortable default loop.
    fn default() -> Self {
        Timeline::new(4, 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_toggles_and_queries_hits() {
        let mut t = Timeline::new(2, 16);
        assert_eq!(t.total_steps(), 32);
        assert!(!t.step(0, 4));
        assert!(t.toggle(0, 4)); // → on
        assert!(t.step(0, 4));
        t.set_step(3, 4, true);
        assert_eq!(t.pads_at(4), vec![0, 3]);
        assert!(!t.toggle(0, 4)); // → off
        assert_eq!(t.pads_at(4), vec![3]);
        t.clear();
        assert!(t.pads_at(4).is_empty());
    }

    #[test]
    fn out_of_range_is_safe() {
        let mut t = Timeline::new(1, 4);
        assert!(!t.step(99, 0));
        t.set_step(99, 99, true); // no panic
        assert!(t.pads_at(99).is_empty());
    }
}
