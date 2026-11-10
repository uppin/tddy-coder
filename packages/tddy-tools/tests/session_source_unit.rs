//! Unit: the order a `tddy-tools` process finds its session in (`#agent-tools 15/19`) — an
//! explicit `--socket`, then the `.tddy/session.json` pointer at or above its cwd, then
//! `TDDY_SOCKET` — and the error that names all three when none does.
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_core::session_pointer::SessionPointerOwner;
use tddy_tools::session_source::{
    resolve_session, ResolvedSession, SessionNotFound, SessionSource, UnreadablePointerCause,
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
            socket: Some(PathBuf::from("/run/tddy/from-arg.sock")),
            repo_dir: None,
            session_dir: None,
            remote: BTreeMap::new(),
            notify: BTreeMap::new(),
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
            socket: Some(PathBuf::from("/run/tddy/from-pointer.sock")),
            repo_dir: Some(PathBuf::from("/srv/checkouts/app")),
            session_dir: Some(PathBuf::from("/var/tddy/sessions/s1")),
            remote: BTreeMap::from([(
                "TDDY_REMOTE_SESSION_ID".to_string(),
                "remote-1".to_string()
            )]),
            notify: BTreeMap::new(),
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
            socket: Some(PathBuf::from("/run/tddy/from-env.sock")),
            repo_dir: None,
            session_dir: Some(PathBuf::from("/var/tddy/sessions/s2")),
            remote: BTreeMap::new(),
            notify: BTreeMap::new(),
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

#[test]
fn a_pointers_notify_map_sets_only_the_names_tddy_tools_notify_reads() {
    // Given — a pointer naming the notify session, and a name notify does not read
    let session = ResolvedSession {
        socket: None,
        repo_dir: Some(PathBuf::from("/srv/checkouts/app")),
        session_dir: None,
        remote: BTreeMap::new(),
        notify: BTreeMap::from([
            ("TDDY_SESSION_ID".to_string(), "s1".to_string()),
            ("PATH".to_string(), "/planted".to_string()),
        ]),
        source: SessionSource::Env,
    };

    // When
    let env = tddy_tools::session_source::session_environment(&session);

    // Then — no socket is adopted, and PATH is not
    assert_eq!(
        env,
        vec![
            (
                "TDDY_REPO_DIR".to_string(),
                "/srv/checkouts/app".to_string()
            ),
            ("TDDY_SESSION_ID".to_string(), "s1".to_string()),
        ]
    );
}

// --- `#agent-tools 21/28`: an owned pointer is refused once its owner or its socket is gone ---

/// A worktree whose pointer names `socket` and is owned by `owner`, if anyone.
fn a_worktree_with_an_owned_pointer(
    socket: &Path,
    owner: Option<SessionPointerOwner>,
) -> AWorktree {
    let worktree = a_worktree_without_a_pointer();
    std::fs::create_dir_all(worktree.dir.path().join(".tddy")).expect(".tddy");
    let mut pointer = serde_json::json!({
        "socket": socket,
        "repo_dir": "/srv/checkouts/app",
    });
    if let Some(owner) = owner {
        pointer["owner"] = serde_json::json!({ "pid": owner.pid, "host": owner.host });
    }
    std::fs::write(worktree.pointer(), pointer.to_string()).expect("pointer");
    worktree
}

/// A socket that exists: a listener bound in its own directory, kept for as long as the value is.
struct ALiveSocket {
    dir: tempfile::TempDir,
    _listener: std::os::unix::net::UnixListener,
}

fn a_live_socket() -> ALiveSocket {
    let dir = tempfile::tempdir().expect("socket dir");
    let listener = std::os::unix::net::UnixListener::bind(dir.path().join("s.sock")).expect("bind");
    ALiveSocket {
        dir,
        _listener: listener,
    }
}

impl ALiveSocket {
    fn path(&self) -> PathBuf {
        self.dir.path().join("s.sock")
    }
}

/// A process on this host that has exited and been reaped.
fn an_exited_process_on_this_host() -> SessionPointerOwner {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    child.wait().expect("reap");
    SessionPointerOwner {
        pid,
        host: SessionPointerOwner::this_process().host,
    }
}

/// An owner on an ssh target: a pid this host knows nothing of.
fn an_owner_on_another_host() -> SessionPointerOwner {
    SessionPointerOwner {
        pid: an_exited_process_on_this_host().pid,
        host: "buildbox.example".to_string(),
    }
}

#[test]
fn a_pointer_whose_owner_is_gone_is_an_error_naming_the_pointer() {
    // Given — a pointer whose socket is still there, owned by a process here that has exited
    let socket = a_live_socket();
    let gone = an_exited_process_on_this_host();
    let worktree = a_worktree_with_an_owned_pointer(&socket.path(), Some(gone.clone()));
    let env = an_env(&[("TDDY_SOCKET", "/run/tddy/from-env.sock")]);

    // When
    let refused = resolve_session(None, &worktree.nested(), &env);

    // Then — refused, never skipped for TDDY_SOCKET, and the message names the pointer
    let message = format!(
        "the session pointer {} could not be read: the session that wrote it (pid {} on {}) is \
         no longer running",
        worktree.pointer().display(),
        gone.pid,
        gone.host
    );
    assert_eq!(
        refused.map_err(|e| (e.to_string(), e)),
        Err((
            message,
            SessionNotFound::UnreadablePointer {
                path: worktree.pointer(),
                cause: UnreadablePointerCause::OwnerGone { owner: gone },
            }
        ))
    );
}

#[test]
fn a_pointer_naming_a_missing_socket_is_an_error_but_an_explicit_socket_is_not_prechecked() {
    // Given — a pointer owned by this (live) process, naming a socket that does not exist
    let missing = PathBuf::from("/run/tddy/no-such-session.sock");
    let worktree =
        a_worktree_with_an_owned_pointer(&missing, Some(SessionPointerOwner::this_process()));

    // When
    let through_the_pointer = resolve_session(None, &worktree.nested(), &an_env(&[]));
    let through_the_argument = resolve_session(Some(&missing), &worktree.nested(), &an_env(&[]));

    // Then
    assert_eq!(
        (
            through_the_pointer,
            through_the_argument.map(|session| session.source)
        ),
        (
            Err(SessionNotFound::UnreadablePointer {
                path: worktree.pointer(),
                cause: UnreadablePointerCause::SocketMissing { socket: missing },
            }),
            Ok(SessionSource::Arg)
        )
    );
}

#[test]
fn a_pointer_without_an_owner_still_resolves() {
    // Given — a pointer an older daemon wrote: no owner, and a socket this host does not have
    let socket = PathBuf::from("/run/tddy/older-daemon.sock");
    let worktree = a_worktree_with_an_owned_pointer(&socket, None);

    // When
    let session = resolve_session(None, &worktree.nested(), &an_env(&[]));

    // Then
    assert_eq!(
        session.map(|session| (session.socket, session.source)),
        Ok((Some(socket), SessionSource::PointerFile(worktree.pointer())))
    );
}

#[test]
fn a_pointer_owned_on_another_host_is_resolved_without_checking_its_pid_or_socket() {
    // Given — an ssh target's pointer: its owner's pid and its socket path are that host's
    let socket = PathBuf::from("/run/tddy/on-the-target.sock");
    let worktree = a_worktree_with_an_owned_pointer(&socket, Some(an_owner_on_another_host()));

    // When
    let session = resolve_session(None, &worktree.nested(), &an_env(&[]));

    // Then
    assert_eq!(
        session.map(|session| (session.socket, session.source)),
        Ok((Some(socket), SessionSource::PointerFile(worktree.pointer())))
    );
}
