//! The beat-mark cache: each track's tapped beats, kept for good.
//!
//! Beat marks are the engine's only required input besides track order (see
//! the 2026-06-11 auto-mix pivot in `.am/inception.md`), and a track is
//! tapped **once, ever** — so the marks persist next to the user config and
//! follow the track through renames and moves.
//!
//! Marks are stored with the sample rate they were tapped at: a different
//! output device on the next launch just rescales them.
//!
//! On-disk format (plain text, one record per line):
//!
//! ```text
//! # termkrush beats v1
//! /music/a.wav<TAB>44100<TAB>1000,23050,45100
//! ```

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// First line of a beats file; bump the suffix on a breaking change.
const HEADER: &str = "# termkrush beats v1";

/// One track's tapped beats: clip-absolute frame positions at `sample_rate`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatMarks {
    /// The rate the frames are expressed in.
    pub sample_rate: u32,
    /// Sorted beat positions, in frames.
    pub frames: Vec<u64>,
}

impl BeatMarks {
    /// The marks re-expressed at `rate` (frames scale linearly). Lossless
    /// when the rates match.
    pub fn at_rate(&self, rate: u32) -> Vec<u64> {
        if rate == self.sample_rate || self.sample_rate == 0 {
            return self.frames.clone();
        }
        let k = rate as f64 / self.sample_rate as f64;
        self.frames
            .iter()
            .map(|&f| (f as f64 * k).round() as u64)
            .collect()
    }
}

/// The cache: track path → tapped beats. A `BTreeMap` so the saved file has
/// a stable order (no hash-iteration nondeterminism in anything we write).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BeatCache {
    map: BTreeMap<PathBuf, BeatMarks>,
}

impl BeatCache {
    /// The marks for `track`, if it has been tapped.
    pub fn get(&self, track: &Path) -> Option<&BeatMarks> {
        self.map.get(track)
    }

    /// Does `track` have enough marks to fit a grid (two or more)?
    pub fn has_beats(&self, track: &Path) -> bool {
        self.map.get(track).is_some_and(|m| m.frames.len() >= 2)
    }

    /// Store `track`'s marks (sorted on the way in). Empty marks remove the
    /// record — clearing every tap un-taps the track.
    pub fn set(&mut self, track: &Path, sample_rate: u32, mut frames: Vec<u64>) {
        if frames.is_empty() {
            self.map.remove(track);
            return;
        }
        frames.sort_unstable();
        self.map.insert(
            track.to_path_buf(),
            BeatMarks {
                sample_rate,
                frames,
            },
        );
    }

    /// Follow a rename/move: the marks belong to the file, not the old path.
    pub fn retarget(&mut self, old: &Path, new: &Path) {
        if let Some(m) = self.map.remove(old) {
            self.map.insert(new.to_path_buf(), m);
        }
    }

    /// Drop a deleted track's marks.
    pub fn purge(&mut self, track: &Path) {
        self.map.remove(track);
    }

    /// Write the cache to `path` (creating parent directories).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut text = String::from(HEADER);
        text.push('\n');
        for (track, m) in &self.map {
            let frames: Vec<String> = m.frames.iter().map(u64::to_string).collect();
            text.push_str(&format!(
                "{}\t{}\t{}\n",
                track.to_string_lossy(),
                m.sample_rate,
                frames.join(",")
            ));
        }
        std::fs::write(path, text)
    }

    /// Read a cache from `path`. Missing file → empty; a bad header or a
    /// malformed line is logged and skipped, never fatal.
    pub fn load(path: &Path) -> BeatCache {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return BeatCache::default(),
        };
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HEADER) {
            tracing::warn!(path = %path.display(), "beats: unrecognized header, starting empty");
            return BeatCache::default();
        }
        let mut cache = BeatCache::default();
        for line in lines.map(str::trim).filter(|l| !l.is_empty()) {
            let mut parts = line.splitn(3, '\t');
            let (Some(track), Some(sr), Some(frames)) = (parts.next(), parts.next(), parts.next())
            else {
                tracing::warn!(line, "beats: malformed record skipped");
                continue;
            };
            let Ok(sample_rate) = sr.parse::<u32>() else {
                tracing::warn!(line, "beats: bad sample rate skipped");
                continue;
            };
            let frames: Vec<u64> = frames
                .split(',')
                .filter_map(|f| f.parse::<u64>().ok())
                .collect();
            cache.set(Path::new(track), sample_rate, frames);
        }
        cache
    }
}

/// Default cache location: `beats.txt` next to the user config
/// (`$XDG_CONFIG_HOME/termkrush/` or `~/.config/termkrush/`). `None` if the
/// home directory is unknown.
pub fn beats_path() -> Option<PathBuf> {
    crate::config::config_path().map(|p| p.with_file_name("beats.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn set_get_and_unset() {
        let mut c = BeatCache::default();
        c.set(&p("/m/a.wav"), 44_100, vec![300, 100, 200]); // unsorted on purpose
        assert_eq!(c.get(&p("/m/a.wav")).unwrap().frames, vec![100, 200, 300]);
        assert!(c.has_beats(&p("/m/a.wav")));
        assert!(!c.has_beats(&p("/m/other.wav")));

        // One mark can't make a grid.
        c.set(&p("/m/one.wav"), 44_100, vec![5]);
        assert!(!c.has_beats(&p("/m/one.wav")));

        // Clearing the taps removes the record.
        c.set(&p("/m/a.wav"), 44_100, vec![]);
        assert!(c.get(&p("/m/a.wav")).is_none());
    }

    #[test]
    fn retarget_follows_rename_and_purge_drops() {
        let mut c = BeatCache::default();
        c.set(&p("/m/a.wav"), 44_100, vec![10, 20]);
        c.retarget(&p("/m/a.wav"), &p("/m/sub/renamed.wav"));
        assert!(c.get(&p("/m/a.wav")).is_none());
        assert!(c.has_beats(&p("/m/sub/renamed.wav")));

        c.purge(&p("/m/sub/renamed.wav"));
        assert!(c.get(&p("/m/sub/renamed.wav")).is_none());
    }

    #[test]
    fn rescale_to_another_device_rate() {
        let m = BeatMarks {
            sample_rate: 44_100,
            frames: vec![44_100, 88_200],
        };
        assert_eq!(m.at_rate(22_050), vec![22_050, 44_100]);
        assert_eq!(m.at_rate(44_100), vec![44_100, 88_200], "same rate: as-is");
    }

    #[test]
    fn save_load_round_trips() {
        let tmp = std::env::temp_dir().join(format!("tk-beats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let file = tmp.join("beats.txt");

        let mut c = BeatCache::default();
        c.set(&p("/m/a.wav"), 44_100, vec![100, 200, 300]);
        c.set(&p("/m/b second take.mp3"), 48_000, vec![5, 9]); // spaces in names
        c.save(&file).unwrap();

        assert_eq!(BeatCache::load(&file), c);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn load_missing_or_malformed_is_safe() {
        assert_eq!(
            BeatCache::load(Path::new("/no/such/beats.txt")),
            BeatCache::default()
        );

        let tmp = std::env::temp_dir().join(format!("tk-beats-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("beats.txt");
        // Good header, one malformed line, one good line: the good one loads.
        std::fs::write(
            &file,
            "# termkrush beats v1\ngarbage line\n/m/ok.wav\t44100\t7,8\n",
        )
        .unwrap();
        let c = BeatCache::load(&file);
        assert!(c.has_beats(Path::new("/m/ok.wav")));
        assert_eq!(c.map.len(), 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
