//! The sequence: the product's only arranging surface (see the 2026-06-11
//! auto-mix pivot in `.am/inception.md`).
//!
//! An ordered list of track paths — the same track may appear at any number
//! of positions. The sequence (plus each track's cached beat marks) *is* the
//! project: the engine renders the mix from nothing else, so saving the
//! sequence saves the project.
//!
//! The on-disk format is deliberately plain: a version header, then one
//! absolute track path per line. Human-readable, diff-able, and free of any
//! serialization dependency.

use std::io;
use std::path::{Path, PathBuf};

/// First line of a sequence file; bump the suffix on a breaking change.
const HEADER: &str = "# termkrush sequence v1";

/// The ordered track sequence. Entries are track paths; repeats are allowed
/// (entry 1 and entry 5 may be the same file).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sequence {
    entries: Vec<PathBuf>,
}

impl Sequence {
    /// The entries in play order.
    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert `track` so it plays at position `idx` (clamped to the end).
    pub fn insert(&mut self, idx: usize, track: PathBuf) {
        let idx = idx.min(self.entries.len());
        self.entries.insert(idx, track);
    }

    /// Append `track` at the end.
    pub fn push(&mut self, track: PathBuf) {
        self.entries.push(track);
    }

    /// Remove the entry at `idx` (out of range is a no-op).
    pub fn remove(&mut self, idx: usize) {
        if idx < self.entries.len() {
            self.entries.remove(idx);
        }
    }

    /// Move the entry at `from` so it sits at `to` (both in current positions;
    /// out-of-range indices are a no-op).
    pub fn move_entry(&mut self, from: usize, to: usize) {
        if from >= self.entries.len() || to >= self.entries.len() {
            return;
        }
        let e = self.entries.remove(from);
        self.entries.insert(to, e);
    }

    /// Point every entry at `old` to `new` — keeps the sequence intact when a
    /// track is renamed or moved in the library.
    pub fn retarget(&mut self, old: &Path, new: &Path) {
        for e in &mut self.entries {
            if e == old {
                *e = new.to_path_buf();
            }
        }
    }

    /// Drop every entry pointing at `track` (after a library delete).
    pub fn purge(&mut self, track: &Path) {
        self.entries.retain(|e| e != track);
    }

    /// Write the sequence to `path` (creating parent directories).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut text = String::from(HEADER);
        text.push('\n');
        for e in &self.entries {
            text.push_str(&e.to_string_lossy());
            text.push('\n');
        }
        std::fs::write(path, text)
    }

    /// Read a sequence from `path`. A missing file is an empty sequence (a
    /// fresh install has no project yet); a malformed header is logged and
    /// treated the same so a damaged file never stops the app.
    pub fn load(path: &Path) -> Sequence {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return Sequence::default(),
        };
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HEADER) {
            tracing::warn!(path = %path.display(), "sequence: unrecognized header, starting empty");
            return Sequence::default();
        }
        let entries = lines
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect();
        Sequence { entries }
    }
}

/// Default project-file location: `sequence.txt` next to the user config
/// (`$XDG_CONFIG_HOME/termkrush/` or `~/.config/termkrush/`). `None` if the
/// home directory is unknown.
pub fn sequence_path() -> Option<PathBuf> {
    crate::config::config_path().map(|p| p.with_file_name("sequence.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn insert_remove_and_repeats() {
        let mut s = Sequence::default();
        s.push(p("/m/a.wav"));
        s.push(p("/m/b.wav"));
        s.insert(1, p("/m/a.wav")); // the same track repeats at position 1
        assert_eq!(s.entries(), &[p("/m/a.wav"), p("/m/a.wav"), p("/m/b.wav")]);

        s.remove(0);
        assert_eq!(s.entries(), &[p("/m/a.wav"), p("/m/b.wav")]);
        s.remove(99); // out of range: no-op
        assert_eq!(s.len(), 2);

        s.insert(99, p("/m/c.wav")); // clamped to the end
        assert_eq!(s.entries()[2], p("/m/c.wav"));
    }

    #[test]
    fn move_entry_reorders() {
        let mut s = Sequence::default();
        for n in ["a", "b", "c", "d"] {
            s.push(p(&format!("/m/{n}.wav")));
        }
        s.move_entry(3, 0); // d to the front
        assert_eq!(
            s.entries(),
            &[p("/m/d.wav"), p("/m/a.wav"), p("/m/b.wav"), p("/m/c.wav")]
        );
        s.move_entry(0, 2); // and back into the middle
        assert_eq!(
            s.entries(),
            &[p("/m/a.wav"), p("/m/b.wav"), p("/m/d.wav"), p("/m/c.wav")]
        );
        s.move_entry(9, 0); // out of range: no-op
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn retarget_updates_every_repeat_and_purge_drops_them() {
        let mut s = Sequence::default();
        s.push(p("/m/a.wav"));
        s.push(p("/m/b.wav"));
        s.push(p("/m/a.wav"));
        s.retarget(&p("/m/a.wav"), &p("/m/renamed.wav"));
        assert_eq!(
            s.entries(),
            &[p("/m/renamed.wav"), p("/m/b.wav"), p("/m/renamed.wav")]
        );

        s.purge(&p("/m/renamed.wav"));
        assert_eq!(s.entries(), &[p("/m/b.wav")]);
    }

    #[test]
    fn save_load_round_trips_order_and_repeats() {
        let tmp = std::env::temp_dir().join(format!("tk-seq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let file = tmp.join("nested").join("sequence.txt");

        let mut s = Sequence::default();
        s.push(p("/m/one.wav"));
        s.push(p("/m/two.mp3"));
        s.push(p("/m/one.wav")); // repeat survives the trip
        s.save(&file).unwrap();

        let loaded = Sequence::load(&file);
        assert_eq!(loaded, s);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn load_missing_or_malformed_is_empty() {
        assert!(Sequence::load(Path::new("/no/such/sequence.txt")).is_empty());

        let tmp = std::env::temp_dir().join(format!("tk-seq-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("sequence.txt");
        std::fs::write(&file, "not a sequence file\n/m/a.wav\n").unwrap();
        assert!(Sequence::load(&file).is_empty(), "bad header starts empty");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
