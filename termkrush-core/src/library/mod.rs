//! Track library: the local list of audio files the TUI browses and pads
//! load from — filesystem-managed, with folders.
//!
//! The library shows one directory at a time under a configured root:
//! subfolders first, then audio files (`.wav` / `.mp3`). You navigate into a
//! folder and back up via the `..` entry; the root is the ceiling. Files move
//! in and out of the directory normally — nothing special in the UI.

use std::path::{Path, PathBuf};

/// One entry in the current directory — a subfolder or an audio file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateEntry {
    /// Path to the file (or directory).
    pub path: PathBuf,
    /// Name for display (a folder, the `..` up-entry, or a file name).
    pub name: String,
    /// `true` for a directory (navigable), `false` for a track.
    pub is_dir: bool,
}

/// The library, scoped to a `root`, showing the current directory `cwd`.
#[derive(Debug, Clone, Default)]
pub struct Crate {
    root: PathBuf,
    cwd: PathBuf,
    entries: Vec<CrateEntry>,
}

impl Crate {
    /// Open the library at `root` (its own directory). A missing/unreadable
    /// directory yields an empty listing rather than an error.
    pub fn scan(root: &Path) -> Crate {
        let root = root.to_path_buf();
        let entries = list_dir(&root, &root);
        Crate {
            cwd: root.clone(),
            root,
            entries,
        }
    }

    /// Build from a known list of entries (tests / non-filesystem sources).
    pub fn from_entries(entries: Vec<CrateEntry>) -> Self {
        Crate {
            root: PathBuf::new(),
            cwd: PathBuf::new(),
            entries,
        }
    }

    /// Navigate into `path` (a directory at or under the root) and relist.
    /// The `..` entry's path is the parent, so the same call walks up.
    pub fn enter(&mut self, path: &Path) {
        if path.is_dir() && path.starts_with(&self.root) {
            self.cwd = path.to_path_buf();
            self.entries = list_dir(&self.cwd, &self.root);
        }
    }

    /// The directory currently shown.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// All entries in the current directory (`..`, folders, then tracks).
    pub fn entries(&self) -> &[CrateEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries whose name fuzzy-matches `query` (case-insensitive
    /// subsequence), preserving order. An empty query matches all.
    pub fn filtered(&self, query: &str) -> Vec<&CrateEntry> {
        self.entries
            .iter()
            .filter(|e| fuzzy_subsequence(&e.name, query))
            .collect()
    }
}

/// List one directory: subfolders + audio files, plus a leading `..` when
/// below the root. Folders sort before tracks, each alphabetically.
fn list_dir(dir: &Path, root: &Path) -> Vec<CrateEntry> {
    let (mut dirs, mut tracks) = (Vec::new(), Vec::new());
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(String::from) else {
                continue;
            };
            if path.is_dir() {
                dirs.push(CrateEntry {
                    path,
                    name,
                    is_dir: true,
                });
            } else if is_audio(&path) {
                tracks.push(CrateEntry {
                    path,
                    name,
                    is_dir: false,
                });
            }
        }
    }
    dirs.sort_by_key(|e| e.name.to_lowercase());
    tracks.sort_by_key(|e| e.name.to_lowercase());

    let mut out = Vec::new();
    if dir != root {
        if let Some(parent) = dir.parent() {
            out.push(CrateEntry {
                path: parent.to_path_buf(),
                name: "..".to_string(),
                is_dir: true,
            });
        }
    }
    out.extend(dirs);
    out.extend(tracks);
    out
}

/// Does `path` have a supported audio extension (`.wav` / `.mp3`, any case)?
fn is_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("wav") | Some("mp3")
    )
}

/// Case-insensitive subsequence match: every char of `needle` appears in
/// `haystack` in order. An empty needle matches everything.
pub fn fuzzy_subsequence(haystack: &str, needle: &str) -> bool {
    let mut hay = haystack.chars().flat_map(char::to_lowercase);
    'outer: for nc in needle.chars().flat_map(char::to_lowercase) {
        for hc in hay.by_ref() {
            if hc == nc {
                continue 'outer;
            }
        }
        return false;
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
        assert!(fuzzy_subsequence("anything", ""));
        assert!(!fuzzy_subsequence("abc", "abcd"));
        assert!(!fuzzy_subsequence("hello", "world"));
    }

    #[test]
    fn lists_folders_then_audio_and_navigates() {
        let tmp = std::env::temp_dir().join(format!("tk-lib-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("sub")).unwrap();
        fs::write(tmp.join("Beta.mp3"), b"x").unwrap();
        fs::write(tmp.join("alpha.WAV"), b"x").unwrap(); // wav, case-insensitive
        fs::write(tmp.join("notes.txt"), b"x").unwrap(); // ignored
        fs::write(tmp.join("sub").join("Gamma.mp3"), b"x").unwrap();

        let mut lib = Crate::scan(&tmp);
        let names: Vec<&str> = lib.entries().iter().map(|e| e.name.as_str()).collect();
        // Folder first, then audio (wav + mp3) alpha; .txt skipped; no `..` at root.
        assert_eq!(names, vec!["sub", "alpha.WAV", "Beta.mp3"]);

        // Enter the subfolder: shows `..` then its track.
        let sub = lib.entries()[0].path.clone();
        lib.enter(&sub);
        let names: Vec<&str> = lib.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["..", "Gamma.mp3"]);
        assert_eq!(lib.cwd(), tmp.join("sub"));

        // The `..` entry walks back to root.
        let up = lib.entries()[0].path.clone();
        lib.enter(&up);
        assert_eq!(lib.cwd(), tmp);

        // Can't escape above the root.
        let outside = tmp.parent().unwrap().to_path_buf();
        lib.enter(&outside);
        assert_eq!(lib.cwd(), tmp, "root is the ceiling");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scan_missing_root_is_empty() {
        let lib = Crate::scan(Path::new("/no/such/termkrush/lib/xyz"));
        assert!(lib.is_empty());
    }
}
