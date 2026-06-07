//! Track library: the local "crate" of mp3 files the TUI browses and
//! pads load from.
//!
//! For now the crate is a recursive scan of a root directory for `*.mp3`.
//! Downloads (direct URL, `yt-dlp`) and cached analysis (BPM, duration)
//! land in their own later stories; this module is the list the browser
//! shows and `enter` loads from.

use std::path::{Path, PathBuf};

/// One track in the crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateEntry {
    /// Absolute (or root-relative) path to the file.
    pub path: PathBuf,
    /// File name without the directory, for display.
    pub name: String,
}

/// The local crate: the mp3 files found under a root, sorted by name.
#[derive(Debug, Clone, Default)]
pub struct Crate {
    entries: Vec<CrateEntry>,
}

impl Crate {
    /// Recursively scan `root` for `*.mp3` (case-insensitive), sorted by
    /// file name. A missing or unreadable root yields an empty crate
    /// rather than an error, so a fresh install just shows nothing.
    pub fn scan(root: &Path) -> Crate {
        let mut entries = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if is_mp3(&path) {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        entries.push(CrateEntry {
                            name: name.to_string(),
                            path: path.clone(),
                        });
                    }
                }
            }
        }
        entries.sort_by_key(|e| e.name.to_lowercase());
        Crate { entries }
    }

    /// Build a crate from a known list of entries (caller-ordered). For
    /// tests and any non-scan source.
    pub fn from_entries(entries: Vec<CrateEntry>) -> Self {
        Crate { entries }
    }

    /// All entries in scan order.
    pub fn entries(&self) -> &[CrateEntry] {
        &self.entries
    }

    /// Number of tracks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when no tracks were found.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries whose name fuzzy-matches `query` (case-insensitive
    /// subsequence), preserving scan order. An empty query matches all.
    pub fn filtered(&self, query: &str) -> Vec<&CrateEntry> {
        self.entries
            .iter()
            .filter(|e| fuzzy_subsequence(&e.name, query))
            .collect()
    }
}

/// Does `path` have an `.mp3` extension (any case)?
fn is_mp3(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mp3"))
        .unwrap_or(false)
}

/// Case-insensitive subsequence match: every char of `needle` appears in
/// `haystack` in order (not necessarily contiguously). An empty needle
/// matches everything. This is the "fuzzy filter" the `/` search uses.
pub fn fuzzy_subsequence(haystack: &str, needle: &str) -> bool {
    let mut hay = haystack.chars().flat_map(char::to_lowercase);
    'outer: for nc in needle.chars().flat_map(char::to_lowercase) {
        for hc in hay.by_ref() {
            if hc == nc {
                continue 'outer;
            }
        }
        return false; // ran out of haystack before matching `nc`
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn fuzzy_matches_subsequence_case_insensitively() {
        assert!(fuzzy_subsequence("Daft Punk - Around.mp3", "dpa"));
        assert!(fuzzy_subsequence("Around the World", "around"));
        assert!(fuzzy_subsequence("anything", "")); // empty matches all
        assert!(!fuzzy_subsequence("abc", "abcd")); // needle longer / not present
        assert!(!fuzzy_subsequence("hello", "world"));
    }

    #[test]
    fn scan_finds_mp3s_recursively_sorted_and_skips_other_files() {
        let tmp = std::env::temp_dir().join(format!("tk-crate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("sub")).unwrap();
        fs::write(tmp.join("Beta.mp3"), b"x").unwrap();
        fs::write(tmp.join("alpha.MP3"), b"x").unwrap(); // case-insensitive ext
        fs::write(tmp.join("notes.txt"), b"x").unwrap(); // ignored
        fs::write(tmp.join("sub").join("Gamma.mp3"), b"x").unwrap();

        let crate_ = Crate::scan(&tmp);
        let names: Vec<&str> = crate_.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["alpha.MP3", "Beta.mp3", "Gamma.mp3"],
            "sorted, recursive, mp3-only"
        );

        // Filter narrows.
        assert_eq!(crate_.filtered("gam").len(), 1);
        assert_eq!(crate_.filtered("").len(), 3);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scan_missing_root_is_empty() {
        let crate_ = Crate::scan(Path::new("/no/such/termkrush/crate/xyz"));
        assert!(crate_.is_empty());
        assert_eq!(crate_.len(), 0);
    }
}
