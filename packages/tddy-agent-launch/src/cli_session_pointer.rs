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

use crate::slot_file_over_ssh::{run_over_ssh, SLOT_FILE_OVER_SSH_DEADLINE};
use crate::spec_launch::SpecLaunchError;
use tddy_core::ssh_exec::shell_single_quote;

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
        // The daemon that launched the agent owns the pointer; on an ssh target the agent's own
        // pid replaces it once the agent has started ([`record_remote_owner`]).
        owner: Some(SessionPointerOwner::this_process()),
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
        run_over_ssh(
            ssh,
            host,
            &path,
            &place_command(worktree, &path),
            &contents,
            SLOT_FILE_OVER_SSH_DEADLINE,
        )
        .map_err(|error| ssh_status(&error, host, "write the session pointer"))?;
        return Ok(Some(PlacedSessionPointer {
            path,
            host: PointerHost::Ssh {
                ssh: ssh.clone(),
                alias: host.clone(),
            },
        }));
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

/// The shell command that places the pointer file `path`, whose content is its stdin, on an ssh
/// target: the pointer is added to the checkout's `info/exclude` unless it is there, and written
/// `umask 077`. A `worktree` that is no checkout has nothing to exclude it from.
fn place_command(worktree: &Path, path: &Path) -> String {
    let pattern = format!("/{SESSION_POINTER_FILE}");
    let dir = path.parent().unwrap_or(worktree);
    format!(
        "set -e; umask 077; \
         if exclude=$(git -C {worktree} rev-parse --path-format=absolute --git-path info/exclude 2>/dev/null); then \
         mkdir -p \"$(dirname \"$exclude\")\"; \
         grep -qxF {pattern} \"$exclude\" 2>/dev/null || echo {pattern} >> \"$exclude\"; fi; \
         mkdir -p {dir} && cat > {file}",
        worktree = shell_single_quote(&worktree.to_string_lossy()),
        pattern = shell_single_quote(&pattern),
        dir = shell_single_quote(&dir.to_string_lossy()),
        file = shell_single_quote(&path.to_string_lossy()),
    )
}

/// The status a failed bounded ssh command on `host` is reported as.
fn ssh_status(error: &SpecLaunchError, host: &str, doing: &str) -> Status {
    match error {
        SpecLaunchError::SlotFileOverSshTimedOut { .. } => {
            Status::deadline_exceeded(error.to_string())
        }
        other => Status::internal(format!("failed to {doing} on {host}: {other}")),
    }
}

/// Where an ssh launch's agent records its pid on the target, relative to its worktree, for
/// [`record_remote_owner`] to read (and remove). The launch writes it as `$$` before it `exec`s the
/// agent, so it is the agent's own pid.
// `record_remote_owner` has no caller until an ssh agent launch exists.
#[allow(dead_code)]
pub(crate) const REMOTE_AGENT_PID_FILE: &str = ".tddy/agent.pid";

/// Once the agent has started on the ssh target `placed` is on: one bounded ssh command reads the
/// target's own name for itself (`hostname`) and the agent's pid ([`REMOTE_AGENT_PID_FILE`], which
/// it removes), and rewrites the placed `pointer` owner-only with that owner, which it returns. A
/// pointer on this host is refused: the daemon that placed it is its owner.
// No caller until an ssh agent launch exists: today an ssh session's agent is not started on the
// target, so there is no pid file to read.
#[allow(dead_code)]
pub(crate) fn record_remote_owner(
    placed: &PlacedSessionPointer,
    pointer: &SessionPointer,
) -> Result<SessionPointerOwner, Status> {
    let PointerHost::Ssh { ssh, alias } = &placed.host else {
        return Err(Status::failed_precondition(format!(
            "the session pointer {} is on this host: the daemon that placed it owns it",
            placed.path.display()
        )));
    };
    // The pointer, without an owner, as compact JSON: the target appends the owner it read.
    let mut unowned = pointer.clone();
    unowned.owner = None;
    let body = serde_json::to_vec(&unowned)
        .map_err(|e| Status::internal(format!("the session pointer cannot be encoded: {e}")))?;
    let worktree = placed
        .path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&placed.path);
    let pid_file = worktree.join(REMOTE_AGENT_PID_FILE);
    let remote = format!(
        "set -e; umask 077; \
         pid=$(cat {pid_file}); rm -f {pid_file}; host=$(hostname); \
         case \"$pid\" in ''|*[!0-9]*) echo \"the agent's pid file holds '$pid'\" >&2; exit 1;; esac; \
         case \"$host\" in ''|*[!A-Za-z0-9._-]*) echo \"the host name '$host' is not a plain name\" >&2; exit 1;; esac; \
         body=$(cat); \
         printf '%s,\"owner\":{{\"pid\":%s,\"host\":\"%s\"}}}}' \"${{body%?}}\" \"$pid\" \"$host\" > {file}.owned; \
         mv {file}.owned {file}; \
         echo \"$pid $host\"",
        pid_file = shell_single_quote(&pid_file.to_string_lossy()),
        file = shell_single_quote(&placed.path.to_string_lossy()),
    );
    let printed = run_over_ssh(
        ssh,
        alias,
        &placed.path,
        &remote,
        &body,
        SLOT_FILE_OVER_SSH_DEADLINE,
    )
    .map_err(|error| ssh_status(&error, alias, "record the agent's owner"))?;
    let unexpected = || {
        Status::internal(format!(
            "the target {alias} answered '{}' to the agent owner lookup",
            printed.trim()
        ))
    };
    let (pid, host) = printed.trim().split_once(' ').ok_or_else(unexpected)?;
    Ok(SessionPointerOwner {
        pid: pid.parse().map_err(|_| unexpected())?,
        host: host.to_string(),
    })
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
        PointerHost::Ssh { ssh, alias } => {
            let removed = run_over_ssh(
                ssh,
                alias,
                &placed.path,
                &format!(
                    "rm -f {}",
                    shell_single_quote(&placed.path.to_string_lossy())
                ),
                &[],
                SLOT_FILE_OVER_SSH_DEADLINE,
            );
            if let Err(e) = removed {
                log::warn!(
                    "could not remove the session pointer {} on {alias}: {e}",
                    placed.path.display()
                );
            }
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
            // An ssh target's removal waits on a bounded ssh: not on this runtime's thread.
            if let Err(e) =
                tokio::task::spawn_blocking(move || remove_placed_pointer(&pointer)).await
            {
                log::warn!("the session pointer's removal did not finish: {e}");
            }
            if let Some(on_exit) = on_exit {
                on_exit(capture).await;
            }
        })
    }))
}

#[cfg(test)]
mod cli_session_pointer_unit_tests;
