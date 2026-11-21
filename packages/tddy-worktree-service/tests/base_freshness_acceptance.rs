//! Acceptance: an orchestrator's home worktree is checked against its trunk's remote tip after a
//! fetch, and fast-forwarded to it; a home with commits of its own is refused and left as it was
//! (`#orchestra` 4/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_worktree_service::base_freshness::{base_freshness, refresh_home, RefreshRefusal};

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to run: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} in {cwd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A bare origin, a clone that pushes to it, and a home worktree detached at origin/main.
struct Trunk {
    _dir: tempfile::TempDir,
    pusher: PathBuf,
    home: PathBuf,
}

impl Trunk {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin.git");
        let pusher = dir.path().join("pusher");
        let main_repo = dir.path().join("main");
        git(
            dir.path(),
            &[
                "init",
                "-q",
                "--bare",
                "-b",
                "main",
                origin.to_str().unwrap(),
            ],
        );
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                pusher.to_str().unwrap(),
            ],
        );
        git(&pusher, &["commit", "-q", "--allow-empty", "-m", "init"]);
        git(&pusher, &["push", "-q", "origin", "HEAD:main"]);
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                main_repo.to_str().unwrap(),
            ],
        );
        let home = dir.path().join("home");
        git(
            &main_repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                home.to_str().unwrap(),
                "origin/main",
            ],
        );
        Self {
            _dir: dir,
            pusher,
            home,
        }
    }

    /// A commit lands on the remote trunk; returns its sha.
    fn lands(&self, subject: &str) -> String {
        git(
            &self.pusher,
            &["commit", "-q", "--allow-empty", "-m", subject],
        );
        git(&self.pusher, &["push", "-q", "origin", "HEAD:main"]);
        git(&self.pusher, &["rev-parse", "HEAD"])
    }

    fn home_head(&self) -> String {
        git(&self.home, &["rev-parse", "HEAD"])
    }
}

#[test]
fn a_current_home_is_zero_behind() {
    // Given a home at the trunk tip
    let trunk = Trunk::new();

    // When its freshness is checked
    let freshness = base_freshness(&trunk.home, "origin/main").expect("freshness");

    // Then it is neither ahead nor behind
    assert_eq!((freshness.ahead, freshness.behind), (0, 0));
    assert_eq!(freshness.home_commit, freshness.remote_tip);
}

#[test]
fn freshness_reports_the_home_behind_after_the_remote_trunk_moves() {
    // Given two commits landed on the remote trunk since the home was cut
    let trunk = Trunk::new();
    trunk.lands("one");
    let tip = trunk.lands("two");

    // When its freshness is checked
    let freshness = base_freshness(&trunk.home, "origin/main").expect("freshness");

    // Then the fetch saw both: the home is two behind the new tip
    assert_eq!(
        (
            freshness.trunk_ref.as_str(),
            freshness.remote_tip.as_str(),
            freshness.behind
        ),
        ("origin/main", tip.as_str(), 2)
    );
}

#[test]
fn refresh_fast_forwards_the_home_to_the_remote_tip() {
    // Given the remote trunk moved
    let trunk = Trunk::new();
    let tip = trunk.lands("landed");

    // When the home is refreshed
    let after = refresh_home(&trunk.home, "origin/main").expect("a fast-forward");

    // Then it is at the tip
    assert_eq!((after.behind, trunk.home_head()), (0, tip));
}

#[test]
fn refresh_refuses_a_diverged_home_and_leaves_it_untouched() {
    // Given a home with a commit of its own, and a remote trunk that moved too
    let trunk = Trunk::new();
    git(
        &trunk.home,
        &["commit", "-q", "--allow-empty", "-m", "local"],
    );
    let before = trunk.home_head();
    trunk.lands("landed");

    // When the home is refreshed
    let refused = refresh_home(&trunk.home, "origin/main");

    // Then it is refused as diverged and the home is where it was
    assert!(
        matches!(refused, Err(RefreshRefusal::Diverged(ref at)) if at.ahead == 1 && at.behind == 1),
        "{refused:?}"
    );
    assert_eq!(trunk.home_head(), before);
}
