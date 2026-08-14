//! Reading the repo-root `.env` a developer points at their on-disk base image.
//!
//! Deliberately hand-rolled rather than a `dotenv` dependency: the semantics needed here
//! are the fifteen lines `./web-dev:38-51` already implements for the same file, and this
//! way the two cannot drift apart while a new dependency is added to the workspace.
//!
//! The rule that matters is the last one — **an already-set variable always wins**. A
//! developer exporting `TDDY_CLOUDINIT_BASE_IMAGE` for one run must not have it silently
//! replaced by whatever their `.env` happens to say.

use std::path::Path;

/// The env var naming the on-disk base image every bake starts from.
///
/// The same knob `tddy-vm-build cloud-init --base-image` reads and the existing `tddy-vm`
/// acceptance tests gate on — not a test-only variable. Nothing here ever downloads an
/// image.
pub const BASE_IMAGE_ENV: &str = "TDDY_CLOUDINIT_BASE_IMAGE";

/// Parse `.env` content into key/value pairs.
///
/// Mirrors `IFS='=' read -r key value`: blank lines and `#` comments are skipped, the
/// first `=` separates key from value, and one layer of surrounding single or double
/// quotes is stripped. A key with an empty value is reported rather than dropped — the
/// caller decides that empty means unset, the same way it treats an empty env var.
pub fn parse_env_file(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_string(), unquote(value).to_string()))
        })
        .collect()
}

/// Strip one layer of matching surrounding quotes, as the shell would have.
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

/// Apply the repo-root `.env` to this process, never overriding a variable that is
/// already set.
///
/// A missing file is not an error: `.env` is gitignored and per-developer, so its absence
/// just means every variable must come from the environment instead.
///
/// # Safety
///
/// Mutates process-global state via [`std::env::set_var`]. Call it once, before any
/// thread reads the environment — the testkit does so from a `OnceLock` for exactly this
/// reason.
pub fn load_repo_env_file(repo_root: &Path) {
    let path = repo_root.join(".env");
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return;
    };
    for (key, value) in parse_env_file(&contents) {
        if std::env::var_os(&key).is_none() {
            // SAFETY: called from `TestkitLayout`'s `OnceLock` initialiser, before the
            // testkit hands any VM handle to a test and therefore before any other thread
            // it owns exists.
            unsafe { std::env::set_var(&key, &value) };
        }
    }
}

/// The configured base image, treating an empty value as unset.
///
/// Empty-as-unset matches `tddy-livekit-testkit`'s handling of `LIVEKIT_TESTKIT_WS_URL`
/// (`livekit_testkit.rs:77-79`): an exported-but-blank variable is a developer clearing
/// it, not a request to open `""`.
pub fn configured_base_image() -> Option<std::path::PathBuf> {
    let raw = std::env::var(BASE_IMAGE_ENV).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| std::path::PathBuf::from(trimmed))
}
