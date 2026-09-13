//! Shared test helpers for `tddy-worktree-service`.
//!
//! The same two constants and the same shape as `tddy_daemon::test_util`, so a test that moved here
//! with its handler reads identically to the one it was. It is a second copy rather than a shared
//! one because `tddy-daemon` depends on this crate and not the other way round, and a test-only
//! dependency back would make the graph cyclic for a `TEST_TOKEN`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::SessionUserResolver;

use crate::service::WorktreeServiceImpl;

/// Token accepted by [`test_service`] as a valid session token.
pub const TEST_TOKEN: &str = "valid-token";
/// GitHub user returned for [`TEST_TOKEN`].
pub const TEST_USER: &str = "testuser";

/// A config mapping [`TEST_USER`] onto `os_user`.
///
/// The OS user is a parameter rather than a constant because half these tests run as whoever is
/// running them: a worktree is a directory the daemon reads as *that* user, so a fixed name would
/// make every filesystem assertion a test of the CI account's existence.
pub fn test_config_for_os_user(os_user: &str) -> DaemonConfig {
    let yaml = format!("users:\n  - github_user: \"{TEST_USER}\"\n    os_user: \"{os_user}\"\n");
    let dir = tempfile::tempdir().expect("create temp dir for test config");
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("write test config");
    DaemonConfig::load(&path).expect("load test config")
}

/// The standard test token resolver: [`TEST_TOKEN`] is [`TEST_USER`], everything else is nobody.
pub fn test_user_resolver() -> SessionUserResolver {
    Arc::new(|token| {
        if token == TEST_TOKEN {
            Some(TEST_USER.to_string())
        } else {
            None
        }
    })
}

/// A [`WorktreeServiceImpl`] rooted at `tddy_data_dir`, serving `os_user`.
pub fn test_service(tddy_data_dir: PathBuf, os_user: &str) -> WorktreeServiceImpl {
    WorktreeServiceImpl::new(
        test_config_for_os_user(os_user),
        tddy_data_dir,
        test_user_resolver(),
    )
}

/// The OS user the process is running as — whose home the git fixtures are actually written under.
pub fn current_os_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "root".to_string())
}

/// Skip-proofing for the git-backed suites: a missing `git` fails loudly rather than silently
/// passing a test that never cut a worktree.
pub fn require_git() {
    let ok = std::process::Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok, "git must be installed to run the worktree suites");
}

/// A path's parent, or the path itself at the filesystem root.
pub fn parent_or_self(path: &Path) -> &Path {
    path.parent().unwrap_or(path)
}
