//! Persistent tool homes: `<tddyhome>/homes/<key>`.
//!
//! Feature: the daemon docs under `docs/ft/daemon/`, § The jail from its tool definition (`#agent-tools 5/12`).
//!
//! A tool declaring `home: { kind: persistent, key: K }` runs with `HOME=<tddyhome>/homes/K`,
//! mounted read-write and shared by every session naming `K`. The directory replaces the
//! tool-named `<tddyhome>/sandbox-K-home` homes the recipes created; the
//! first resolution after the upgrade moves one in place with a `rename`, so its credentials and
//! history are kept and nothing is copied.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// The directory under `<tddyhome>` that holds every persistent home.
pub const HOMES_DIR: &str = "homes";

/// Why a persistent home could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HomeError {
    /// Both the legacy home and the new one exist: which one holds the tool's live state is not
    /// for the daemon to guess, so neither is touched.
    #[error(
        "both {} and {} exist: move the one to keep into {} and remove the other",
        legacy.display(),
        home.display(),
        home.display()
    )]
    BothExist { legacy: PathBuf, home: PathBuf },
    /// Creating or moving the home failed.
    #[error("persistent home {}: {message}", path.display())]
    Io { path: PathBuf, message: String },
}

/// The legacy, tool-named home a recipe created for `key`: `<tddyhome>/sandbox-<key>-home`.
pub fn legacy_home(tddy_home: &Path, key: &str) -> PathBuf {
    tddy_home.join(format!("sandbox-{key}-home"))
}

/// `<tddyhome>/homes/<key>`, created when absent and canonical.
///
/// When only [`legacy_home`] exists it is renamed into place first, once; when both exist the
/// resolution is refused ([`HomeError::BothExist`]) naming both.
pub fn persistent_home(tddy_home: &Path, key: &str) -> Result<PathBuf, HomeError> {
    let home = tddy_home.join(HOMES_DIR).join(key);
    let legacy = legacy_home(tddy_home, key);
    let io_error = |path: &Path, e: std::io::Error| HomeError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    };
    if legacy.exists() {
        if home.exists() {
            return Err(HomeError::BothExist { legacy, home });
        }
        let homes_dir = tddy_home.join(HOMES_DIR);
        std::fs::create_dir_all(&homes_dir).map_err(|e| io_error(&homes_dir, e))?;
        std::fs::rename(&legacy, &home).map_err(|e| io_error(&legacy, e))?;
    } else {
        std::fs::create_dir_all(&home).map_err(|e| io_error(&home, e))?;
    }
    std::fs::canonicalize(&home).map_err(|e| io_error(&home, e))
}
