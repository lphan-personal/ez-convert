//! Where the app keeps disposable state: the job temp area and the undo
//! journal (`ARCHITECTURE.md` §13).
//!
//! Deliberately **not** the config directory: neither the journal nor a job's
//! temp files are user data, and a user who deletes the cache must lose undo
//! history and nothing else. `EZC_CACHE_DIR` overrides the location so tests and
//! probes never touch a real profile (the same escape hatch as
//! `EZC_CONFIG_DIR` in `model::settings`, and not a user-facing setting).

use std::path::PathBuf;

/// The app cache directory, per platform.
pub fn cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("EZC_CACHE_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    platform_cache_dir().join("app.ezconvert")
}

/// `<cache>/journal` — one JSON file per job (`ARCHITECTURE.md` §13).
pub fn journal_dir() -> PathBuf {
    cache_dir().join("journal")
}

/// `<cache>/jobs` — per-job temp area; engines write here, never into the
/// user's folder (`ARCHITECTURE.md` §9.4).
pub fn jobs_dir() -> PathBuf {
    cache_dir().join("jobs")
}

#[cfg(target_os = "windows")]
fn platform_cache_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "macos")]
fn platform_cache_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library").join("Caches"))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_override_is_honoured_and_children_are_relative_to_it() {
        // Env vars are process-global; this test is the only writer of this one
        // in the suite, and it restores what it found.
        let previous = std::env::var("EZC_CACHE_DIR").ok();
        std::env::set_var("EZC_CACHE_DIR", "/tmp/ezc-cache-probe");
        assert_eq!(cache_dir(), PathBuf::from("/tmp/ezc-cache-probe"));
        assert_eq!(journal_dir(), PathBuf::from("/tmp/ezc-cache-probe/journal"));
        assert_eq!(jobs_dir(), PathBuf::from("/tmp/ezc-cache-probe/jobs"));
        match previous {
            Some(v) => std::env::set_var("EZC_CACHE_DIR", v),
            None => std::env::remove_var("EZC_CACHE_DIR"),
        }
    }

    #[test]
    fn the_default_is_under_the_platform_cache_root() {
        let previous = std::env::var("EZC_CACHE_DIR").ok();
        std::env::remove_var("EZC_CACHE_DIR");
        let dir = cache_dir();
        assert!(
            dir.ends_with("app.ezconvert"),
            "cache dir should be the app's own subdirectory, got {}",
            dir.display()
        );
        match previous {
            Some(v) => std::env::set_var("EZC_CACHE_DIR", v),
            None => {}
        }
    }
}
