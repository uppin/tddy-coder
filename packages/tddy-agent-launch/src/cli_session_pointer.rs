//! The session pointer of a daemon-launched CLI agent (`#agent-tools 17/19`): how a `tddy-tools`
//! the agent runs from its own shell — `tddy-tools submit`, `transition`, `ask`, `notify` — finds its
//! session when the agent process is given no `TDDY_*` variable.
//!
//! The generic CLI launch hands a managed workflow's `TDDY_SOCKET` (with `TDDY_SESSION_DIR` and
//! `TDDY_REPO_DIR`) to the agent's tool server registration only, and the operator-notify session
//! (`session_activity::notify_env`) to nobody. Both are written instead as the `.tddy/session.json`
//! pointer n15 introduced for headless agents ([`tddy_core::session_pointer::SessionPointer`]), in
//! the session's worktree, git-excluded through `info/exclude` when the worktree is a checkout
//! ([`tddy_core::turn_files::exclude_from_git`]). `tddy-tools` walks up from its cwd to it
//! (`packages/tddy-tools/docs/session-resolution.md`).
//!
//! Lifecycle: written at every start and resume (a stale one is replaced), kept while the agent
//! runs, removed when the agent exits ([`removing_session_pointer_on_exit`]) and when the session is
//! deleted ([`remove_session_pointer`]). An ssh session's pointer is written on its target through
//! the bounded slot-file write. A jailed session writes none: its agent's shell commands run on this
//! host through the Shell relay, whose environment already carries the session
//! (`JailedSession::session_env`), and a pointer copied into the jail would name a host socket the
//! jail cannot reach.

use std::path::{Path, PathBuf};

use tddy_cli_sessions::cli_session_manager::ExitHook;
use tddy_core::session_pointer::{SessionPointer, SessionPointerOwner, SESSION_POINTER_FILE};
use tddy_rpc::Status;

use crate::slot_file_over_ssh::{write_over_ssh, SLOT_FILE_OVER_SSH_DEADLINE};
use crate::spec_launch::SpecLaunchError;

/// The pointer holds the agent's hook token: owner-only, as a headless turn writes it.
const OWNER_ONLY_FILE: u32 = 0o600;

/// The pointer of a launch in `worktree` whose wiring gave its tool server `tool_server_env` (a
/// managed workflow's `TDDY_SOCKET`, `TDDY_SESSION_DIR`, `TDDY_REPO_DIR`) and its operator notify
/// `notify`. `None` when neither names anything: the agent's `tddy-tools` has no session to reach.
pub(crate) fn session_pointer_of(
    tool_server_env: &[(String, String)],
    notify: Vec<(String, String)>,
    worktree: &Path,
) -> Option<SessionPointer> {
    let named = |name: &str| {
        tool_server_env
            .iter()
            .find(|(set, value)| set == name && !value.is_empty())
            .map(|(_, value)| PathBuf::from(value))
    };
    let socket = named("TDDY_SOCKET");
    if socket.is_none() && notify.is_empty() {
        return None;
    }
    Some(SessionPointer {
        socket,
        repo_dir: named("TDDY_REPO_DIR").unwrap_or_else(|| worktree.to_path_buf()),
        session_dir: named("TDDY_SESSION_DIR"),
        remote: tool_server_env
            .iter()
            .filter(|(name, _)| name.starts_with("TDDY_REMOTE_"))
            .cloned()
            .collect(),
        notify: notify.into_iter().collect(),
        // TODO(session-pointer): implement — the daemon that launched the agent owns the pointer.
        owner: None,
    })
}

/// The host a session's worktree, and so its pointer, is on (`#agent-tools 21/28`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PointerHost {
    /// This host.
    Local,
    /// The ssh target `alias`, reached through the ssh client `ssh`.
    Ssh { ssh: PathBuf, alias: String },
}

impl PointerHost {
    /// The host of a session whose worktree is on `remote_host` (its `ssh_config_host`), reached
    /// through the `ssh` on `PATH`; this host when it names none.
    pub(crate) fn of(remote_host: Option<&str>) -> Self {
        Self::through(Path::new("ssh"), remote_host)
    }

    /// [`Self::of`], reaching an ssh target through the ssh client `ssh`.
    pub(crate) fn through(ssh: &Path, remote_host: Option<&str>) -> Self {
        match remote_host.filter(|alias| !alias.trim().is_empty()) {
            Some(alias) => PointerHost::Ssh {
                ssh: ssh.to_path_buf(),
                alias: alias.to_string(),
            },
            None => PointerHost::Local,
        }
    }
}

/// A pointer a launch placed, and the host it is on: what its agent's exit removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlacedSessionPointer {
    pub(crate) path: PathBuf,
    pub(crate) host: PointerHost,
}

/// Where `worktree`'s pointer lives.
pub(crate) fn session_pointer_path(worktree: &Path) -> PathBuf {
    worktree.join(SESSION_POINTER_FILE)
}

/// Write `pointer` into `worktree` on `host`, git-excluded and owner-only, returning what to remove
/// when the agent exits. On an ssh target the exclude and the write are one bounded ssh command.
/// `None` writes nothing.
pub(crate) fn place_session_pointer(
    pointer: Option<&SessionPointer>,
    worktree: &Path,
    host: &PointerHost,
) -> Result<Option<PlacedSessionPointer>, Status> {
    let Some(pointer) = pointer else {
        return Ok(None);
    };
    let path = session_pointer_path(worktree);
    let contents = serde_json::to_vec_pretty(pointer)
        .map_err(|e| Status::internal(format!("the session pointer cannot be encoded: {e}")))?;
    if let PointerHost::Ssh { ssh, alias: host } = host {
        // TODO(session-pointer): implement — one bounded ssh command that adds the pointer to the
        // target's `info/exclude` and writes it `umask 077`; returned for removal on exit
        // (docs/dev/todo/2026-11-06-ssh-session-pointer-exclude-and-removal.md).
        write_over_ssh(ssh, host, &path, &contents, SLOT_FILE_OVER_SSH_DEADLINE).map_err(
            |error| match error {
                SpecLaunchError::SlotFileOverSshTimedOut { .. } => {
                    Status::deadline_exceeded(error.to_string())
                }
                other => Status::internal(format!(
                    "failed to write the session pointer on {host}: {other}"
                )),
            },
        )?;
        return Ok(None);
    }
    if inside_a_git_checkout(worktree)? {
        tddy_core::turn_files::exclude_from_git(worktree, &format!("/{SESSION_POINTER_FILE}"))
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| Status::internal(format!("failed to create {}: {e}", dir.display())))?;
    }
    tddy_core::atomic_file::write_atomic_with_mode(&path, contents, OWNER_ONLY_FILE).map_err(
        |e| {
            Status::internal(format!(
                "failed to write the session pointer {}: {e}",
                path.display()
            ))
        },
    )?;
    Ok(Some(PlacedSessionPointer {
        path,
        host: PointerHost::Local,
    }))
}

/// Where an ssh launch's agent records its pid on the target, relative to its worktree, for
/// [`record_remote_owner`] to read (and remove).
// TODO(session-pointer): written by the ssh launch, read by `record_remote_owner`; drop the allow.
#[allow(dead_code)]
pub(crate) const REMOTE_AGENT_PID_FILE: &str = ".tddy/agent.pid";

/// Once the agent has started on the ssh target `placed` is on: one bounded ssh command reads the
/// target's own name for itself (`hostname`) and the agent's pid ([`REMOTE_AGENT_PID_FILE`], which
/// it removes), and rewrites the placed `pointer` owner-only with that owner, which it returns. A
/// pointer on this host is refused: the daemon that placed it is its owner.
// TODO(session-pointer): called by the ssh launch once its agent has started; drop the allow.
#[allow(dead_code)]
pub(crate) fn record_remote_owner(
    placed: &PlacedSessionPointer,
    pointer: &SessionPointer,
) -> Result<SessionPointerOwner, Status> {
    // TODO(session-pointer): implement
    let _ = (placed, pointer);
    Err(Status::unimplemented(
        "the remote agent's owner is not recorded yet",
    ))
}

/// Whether `worktree` lies in a git checkout — one of its own, or one of a directory above it. Only
/// a checkout can commit the pointer, so only a checkout's `info/exclude` needs it: a session whose
/// worktree is no checkout at all (a peer given a plain directory, a resume in its session
/// directory) gets its pointer with nothing to exclude it from. Refused when git cannot be run.
fn inside_a_git_checkout(worktree: &Path) -> Result<bool, Status> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["rev-parse", "--git-common-dir"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|e| {
            Status::internal(format!(
                "git could not be run to keep the session pointer out of commits: {e}"
            ))
        })
}

/// Remove `worktree`'s pointer on `host`, if any: the session is gone.
pub(crate) fn remove_session_pointer(worktree: &Path, host: &PointerHost) {
    remove_placed_pointer(&PlacedSessionPointer {
        path: session_pointer_path(worktree),
        host: host.clone(),
    });
}

/// Remove `placed`, on the host it is on.
fn remove_placed_pointer(placed: &PlacedSessionPointer) {
    match &placed.host {
        PointerHost::Local => remove_pointer_file(&placed.path),
        PointerHost::Ssh { .. } => {
            // TODO(session-pointer): implement — a bounded ssh `rm -f` of the pointer on the target.
        }
    }
}

fn remove_pointer_file(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!(
                "could not remove the session pointer {}: {e}",
                path.display()
            );
        }
    }
}

/// `on_exit`, preceded by the removal of the pointer at `pointer` once the agent has exited. With
/// no pointer `on_exit` is returned unchanged — and so it is for an agent run `in_tmux` without an
/// exit hook of its own: a tmux pane is kept after its agent exits only for a hook that reads it,
/// and the pointer's removal is no reason to keep one. That pointer goes with the session's delete.
pub(crate) fn removing_session_pointer_on_exit(
    on_exit: Option<ExitHook>,
    pointer: Option<PlacedSessionPointer>,
    in_tmux: bool,
) -> Option<ExitHook> {
    let Some(pointer) = pointer else {
        return on_exit;
    };
    if in_tmux && on_exit.is_none() {
        return None;
    }
    Some(Box::new(move |capture| {
        Box::pin(async move {
            remove_placed_pointer(&pointer);
            if let Some(on_exit) = on_exit {
                on_exit(capture).await;
            }
        })
    }))
}

#[cfg(test)]
mod cli_session_pointer_unit_tests;
