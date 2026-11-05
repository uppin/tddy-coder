//! Unit: the order a `tddy-tools` process finds its session in (`#agent-tools 15/19`) — an
//! explicit `--socket`, then the `.tddy/session.json` pointer at or above its cwd, then
//! `TDDY_SOCKET` — and the error that names all three when none does.
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_tools::session_source::{
    resolve_session, ResolvedSession, SessionNotFound, SessionSource,
};

/// A worktree holding a session pointer at its root, and a directory nested below it.
struct AWorktree {
    dir: tempfile::TempDir,
}

fn a_worktree_with_a_pointer() -> AWorktree {
    let dir = tempfile::tempdir().expect("worktree");
    std::fs::create_dir_all(dir.path().join(".tddy")).expect(".tddy");
    std::fs::create_dir_all(dir.path().join("crates/app/src")).expect("nested");
    std::fs::write(
        dir.path().join(".tddy/session.json"),
        serde_json::json!({
            "socket": "/run/tddy/from-pointer.sock",
            "repo_dir": "/srv/checkouts/app",
            "session_dir": "/var/tddy/sessions/s1",
            "remote": { "TDDY_REMOTE_SESSION_ID": "remote-1" },
        })
        .to_string(),
    )
    .expect("pointer");
    AWorktree { dir }
}

fn a_worktree_without_a_pointer() -> AWorktree {
    let dir = tempfile::tempdir().expect("worktree");
    std::fs::create_dir_all(dir.path().join("crates/app/src")).expect("nested");
    AWorktree { dir }
}

impl AWorktree {
    fn nested(&self) -> PathBuf {
        self.dir.path().join("crates/app/src")
    }

    fn pointer(&self) -> PathBuf {
        self.dir.path().join(".tddy/session.json")
    }
}

/// An environment with only `vars`.
fn an_env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let vars: BTreeMap<String, String> = vars
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    move |name| vars.get(name).cloned()
}

#[test]
fn an_explicit_socket_wins_over_the_pointer_and_the_environment() {
    // Given
    let worktree = a_worktree_with_a_pointer();
    let env = an_env(&[("TDDY_SOCKET", "/run/tddy/from-env.sock")]);

    // When
    let session = resolve_session(
        Some(Path::new("/run/tddy/from-arg.sock")),
        &worktree.nested(),
        &env,
    );

    // Then
    assert_eq!(
        session,
        Ok(ResolvedSession {
            socket: PathBuf::from("/run/tddy/from-arg.sock"),
            repo_dir: None,
            session_dir: None,
            remote: BTreeMap::new(),
            source: SessionSource::Arg,
        })
    );
}

#[test]
fn the_pointer_above_the_cwd_wins_over_the_environment() {
    // Given
    let worktree = a_worktree_with_a_pointer();
    let env = an_env(&[("TDDY_SOCKET", "/run/tddy/from-env.sock")]);

    // When
    let session = resolve_session(None, &worktree.nested(), &env);

    // Then
    assert_eq!(
        session,
        Ok(ResolvedSession {
            socket: PathBuf::from("/run/tddy/from-pointer.sock"),
            repo_dir: Some(PathBuf::from("/srv/checkouts/app")),
            session_dir: Some(PathBuf::from("/var/tddy/sessions/s1")),
            remote: BTreeMap::from([(
                "TDDY_REMOTE_SESSION_ID".to_string(),
                "remote-1".to_string()
            )]),
            source: SessionSource::PointerFile(worktree.pointer()),
        })
    );
}

#[test]
fn the_environment_is_read_only_when_no_argument_or_pointer_names_a_session() {
    // Given
    let worktree = a_worktree_without_a_pointer();
    let env = an_env(&[
        ("TDDY_SOCKET", "/run/tddy/from-env.sock"),
        ("TDDY_SESSION_DIR", "/var/tddy/sessions/s2"),
    ]);

    // When
    let session = resolve_session(None, &worktree.nested(), &env);

    // Then
    assert_eq!(
        session,
        Ok(ResolvedSession {
            socket: PathBuf::from("/run/tddy/from-env.sock"),
            repo_dir: None,
            session_dir: Some(PathBuf::from("/var/tddy/sessions/s2")),
            remote: BTreeMap::new(),
            source: SessionSource::Env,
        })
    );
}

#[test]
fn no_session_anywhere_is_an_error_naming_all_three_sources() {
    // Given
    let worktree = a_worktree_without_a_pointer();

    // When
    let missing = resolve_session(None, &worktree.nested(), &an_env(&[]));

    // Then
    assert_eq!(
        missing.map_err(|e| e.to_string()),
        Err(format!(
            "no session to report to: pass --socket <path>, run from a directory at or below a \
             .tddy/session.json (searched upward from {}), or set TDDY_SOCKET",
            worktree.nested().display()
        ))
    );
}

#[test]
fn a_pointer_that_does_not_parse_is_an_error_never_skipped_for_the_environment() {
    // Given
    let worktree = a_worktree_without_a_pointer();
    std::fs::create_dir_all(worktree.dir.path().join(".tddy")).expect(".tddy");
    std::fs::write(worktree.pointer(), "not json").expect("a broken pointer");
    let env = an_env(&[("TDDY_SOCKET", "/run/tddy/from-env.sock")]);

    // When
    let refused = resolve_session(None, &worktree.nested(), &env);

    // Then
    assert_eq!(
        refused.map_err(|e| match e {
            SessionNotFound::UnreadablePointer { path, .. } => Some(path),
            SessionNotFound::Nowhere { .. } => None,
        }),
        Err(Some(worktree.pointer()))
    );
}
