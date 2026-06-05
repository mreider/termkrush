//! User configuration.
//!
//! Settings load from `~/.config/termkrush/config.toml` (or
//! `$XDG_CONFIG_HOME/termkrush/config.toml`), falling back to sensible
//! defaults when the file is missing or a key is absent. Today the only
//! key is the crate root; audio device, key bindings, and palette join it
//! as their stories land.
//!
//! ```toml
//! # ~/.config/termkrush/config.toml
//! crate_root = "~/Music/termkrush"
//! ```

use std::path::PathBuf;

/// Resolved configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Root directory scanned for the local crate (mp3 files).
    pub crate_root: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            crate_root: default_crate_root(),
        }
    }
}

impl Config {
    /// Load from the user config file, falling back to defaults for any
    /// missing file or key. Parse errors are logged and treated as "use
    /// defaults" so a malformed file never stops the app from starting.
    pub fn load() -> Self {
        match config_path() {
            Some(path) if path.exists() => match std::fs::read_to_string(&path) {
                Ok(text) => Self::from_toml(&text),
                Err(e) => {
                    tracing::warn!(error = %e, path = %path.display(), "config: unreadable, using defaults");
                    Config::default()
                }
            },
            _ => Config::default(),
        }
    }

    /// Parse a config from TOML text, filling unknown/missing keys from
    /// defaults. Exposed for tests; tolerant of malformed input.
    pub fn from_toml(text: &str) -> Self {
        let mut cfg = Config::default();
        match text.parse::<toml::Table>() {
            Ok(table) => {
                if let Some(s) = table.get("crate_root").and_then(|v| v.as_str()) {
                    cfg.crate_root = expand_tilde(s);
                }
            }
            Err(e) => tracing::warn!(error = %e, "config: parse error, using defaults"),
        }
        cfg
    }
}

/// Path to the config file: `$XDG_CONFIG_HOME/termkrush/config.toml`, else
/// `~/.config/termkrush/config.toml`. `None` if the home dir is unknown.
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| home_dir().map(|h| h.join(".config")))?;
    Some(base.join("termkrush").join("config.toml"))
}

/// Default crate root: `~/Music/termkrush` (just `Music/termkrush` if the
/// home dir cannot be determined).
pub fn default_crate_root() -> PathBuf {
    match home_dir() {
        Some(h) => h.join("Music").join("termkrush"),
        None => PathBuf::from("Music").join("termkrush"),
    }
}

/// Expand a leading `~/` (or bare `~`) to the home directory. Other paths
/// pass through unchanged.
fn expand_tilde(s: &str) -> PathBuf {
    expand_tilde_with(s, home_dir())
}

/// Tilde expansion against an explicit home (testable without touching the
/// process environment).
fn expand_tilde_with(s: &str, home: Option<PathBuf>) -> PathBuf {
    if s == "~" {
        return home.unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

/// The user's home directory from `$HOME` (Unix) or `%USERPROFILE%`
/// (Windows), avoiding a `dirs`-crate dependency for one lookup.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty() {
        let cfg = Config::from_toml("");
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn reads_crate_root() {
        let cfg = Config::from_toml(r#"crate_root = "/music/dj""#);
        assert_eq!(cfg.crate_root, PathBuf::from("/music/dj"));
    }

    #[test]
    fn expand_tilde_uses_injected_home() {
        // Tested with an explicit home so it never mutates the shared
        // process environment (which would race other tests).
        let home = Some(PathBuf::from("/home/tester"));
        assert_eq!(
            expand_tilde_with("~/Music/set", home.clone()),
            PathBuf::from("/home/tester/Music/set")
        );
        assert_eq!(
            expand_tilde_with("~", home.clone()),
            PathBuf::from("/home/tester")
        );
        assert_eq!(
            expand_tilde_with("/abs/path", home),
            PathBuf::from("/abs/path")
        );
        // No home: the tilde path is left literal.
        assert_eq!(expand_tilde_with("~/x", None), PathBuf::from("~/x"));
    }

    #[test]
    fn malformed_toml_falls_back_to_defaults() {
        let cfg = Config::from_toml("this is = = not toml [[[");
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn unknown_keys_ignored() {
        let cfg = Config::from_toml("volume = 0.8\ntheme = \"amber\"");
        assert_eq!(cfg.crate_root, default_crate_root());
    }
}
