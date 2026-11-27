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
//! the remote-shell port. A jailed session writes none: its agent's shell commands run on this
//! host through the Shell relay, whose environment already carries the session
//! (`JailedSession::session_env`), and a pointer copied into the jail would name a host socket the
//! jail cannot reach.

use std::path::{Path, PathBuf};

use tddy_cli_sessions::cli_session_manager::ExitHook;
use tddy_core::session_pointer::{SessionPointer, SessionPointerOwner, SESSION_POINTER_FILE};
use tddy_rpc::Status;

use tddy_remote_shell::{ExecRequest, RemoteShell, RemoteShellError, SshShell};

use crate::slot_files_over_shell::place_session_pointer_through;
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
#[derive(Clone)]
pub(crate) enum PointerHost {
    /// This host.
    Local,
    /// A target reached through the remote-shell port's ssh transport.
    Ssh { shell: SshShell },
    /// A target reached through the remote-shell port, by whichever transport reaches it.
    Remote {
        shell: std::sync::Arc<dyn RemoteShell>,
    },
}

impl PointerHost {
    /// The host of a session whose worktree is on `remote_host` (its `ssh_config_host`), reached
    /// through the `ssh` on `PATH`; this host when it names none.
    pub(crate) fn of(remote_host: Option<&str>) -> Self {
        match remote_host.filter(|alias| !alias.trim().is_empty()) {
            Some(alias) => PointerHost::Ssh {
                shell: SshShell::new(alias),
            },
            None => PointerHost::Local,
        }
    }

    /// [`Self::of`], reaching an ssh target through the ssh client `ssh`.
    #[allow(dead_code)]
    pub(crate) fn through(ssh: &Path, remote_host: Option<&str>) -> Self {
        match remote_host.filter(|alias| !alias.trim().is_empty()) {
            Some(alias) => PointerHost::Ssh {
                shell: SshShell::through(ssh.to_path_buf(), alias),
            },
            None => PointerHost::Local,
        }
    }
}

/// The status a failed remote-shell call is reported as.
fn remote_status(error: &RemoteShellError) -> Status {
    match error {
        RemoteShellError::TimedOut { .. } => Status::deadline_exceeded(error.to_string()),
        other => Status::internal(other.to_string()),
    }
}

/// A pointer a launch placed, and the host it is on: what its agent's exit removes.
#[derive(Clone)]
pub(crate) struct PlacedSessionPointer {
    pub(crate) path: PathBuf,
    pub(crate) host: PointerHost,
}

/// Where `worktree`'s pointer lives.
pub(crate) fn session_pointer_path(worktree: &Path) -> PathBuf {
    worktree.join(SESSION_POINTER_FILE)
}

/// Write `pointer` into `worktree` on `host`, git-excluded and owner-only, returning what to remove
/// when the agent exits. On an ssh target the pointer is placed through the remote-shell port.
/// `None` writes nothing.
pub(crate) async fn place_session_pointer(
    fs: &dyn tddy_fs::Fs,
    pointer: Option<&SessionPointer>,
    worktree: &Path,
    host: &PointerHost,
) -> Result<Option<PlacedSessionPointer>, Status> {
    let Some(pointer) = pointer else {
        return Ok(None);
    };
    match host {
        PointerHost::Ssh { shell } => {
            let path = place_session_pointer_through(shell, pointer, worktree).await?;
            Ok(Some(PlacedSessionPointer {
                path,
                host: host.clone(),
            }))
        }
        PointerHost::Remote { shell } => {
            let path = place_session_pointer_through(shell.as_ref(), pointer, worktree).await?;
            Ok(Some(PlacedSessionPointer {
                path,
                host: host.clone(),
            }))
        }
        PointerHost::Local => {
            let path = place_session_pointer_locally(fs, pointer, worktree)?;
            Ok(Some(PlacedSessionPointer {
                path,
                host: PointerHost::Local,
            }))
        }
    }
}

/// Write `pointer` into `worktree` on this host, git-excluded and owner-only, returning its path.
pub(crate) fn place_session_pointer_locally(
    fs: &dyn tddy_fs::Fs,
    pointer: &SessionPointer,
    worktree: &Path,
) -> Result<PathBuf, Status> {
    let path = session_pointer_path(worktree);
    let contents = serde_json::to_vec_pretty(pointer)
        .map_err(|e| Status::internal(format!("the session pointer cannot be encoded: {e}")))?;
    if inside_a_git_checkout(worktree)? {
        tddy_core::turn_files::exclude_from_git(worktree, &format!("/{SESSION_POINTER_FILE}"))
            .map_err(|e| Status::internal(e.to_string()))?;
    }
    if let Some(dir) = path.parent() {
        fs.create_dir_all(dir)
            .map_err(|e| Status::internal(format!("failed to create {}: {e}", dir.display())))?;
    }
    fs.write_atomic(&path, &contents, Some(OWNER_ONLY_FILE)).map_err(
        |e| {
            Status::internal(format!(
                "failed to write the session pointer {}: {e}",
                path.display()
            ))
        },
    )?;
    Ok(path)
}

/// Where the launch of `session_id` records its agent's pid on the target, relative to its
/// worktree: `.tddy/agents/<session id>.pid`, one file per session, so agents sharing a worktree
/// each keep their own. The launch writes it as `$$` before it `exec`s the agent, so it is the
/// agent's own pid; [`record_remote_owner`] reads and removes it.
pub fn remote_agent_pid_file(session_id: &str) -> PathBuf {
    Path::new(".tddy/agents").join(format!("{session_id}.pid"))
}

/// Where a target agent's pid is read from on the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemoteAgentPid {
    /// The pid file the launch's command wrote before it `exec`d the agent
    /// ([`remote_agent_pid_file`]), at this path on the target: consumed when the owner is recorded.
    File(PathBuf),
    /// The pane of the tmux session of this name the agent runs in: the agent is the tmux
    /// server's child, so tmux, asked through the target's login shell, names its pid.
    TmuxPane(String),
}

impl RemoteAgentPid {
    /// A shell command that prints the agent's pid on the target, and exits non-zero while there is
    /// none to print.
    pub(crate) fn read_command(&self) -> String {
        match self {
            Self::File(path) => format!("cat {}", shell_single_quote(&path.to_string_lossy())),
            // `=<name>:` — the exact session, its current pane: `=<name>` alone names no pane to
            // `display-message`, which then prints an empty line and succeeds.
            Self::TmuxPane(name) => crate::remote_agent_launch::target_login_run(&format!(
                "exec tmux display-message -p -t {} {}",
                shell_single_quote(&format!("={name}:")),
                shell_single_quote("#{pane_pid}")
            )),
        }
    }

    /// The `pid=…` assignment the owner record runs: the read, and for a pid file its removal.
    fn consume_snippet(&self) -> String {
        match self {
            Self::File(path) => {
                let file = shell_single_quote(&path.to_string_lossy());
                format!("pid=$(cat {file}); rm -f {file};")
            }
            Self::TmuxPane(_) => format!("pid=$({});", self.read_command()),
        }
    }
}

/// Once the agent has started on the target `placed` is on: one bounded command through the port
/// reads the target's own name for itself (`hostname`) and the pid of `session_id`'s agent
/// ([`remote_agent_pid_file`], which it removes), and rewrites the placed `pointer` owner-only
/// with that owner, which it returns. A pointer on this host is refused: the daemon that placed
/// it is its owner.
#[cfg(test)]
pub(crate) async fn record_remote_owner(
    placed: &PlacedSessionPointer,
    pointer: &SessionPointer,
    session_id: &str,
) -> Result<SessionPointerOwner, Status> {
    let worktree = placed
        .path
        .parent()
        .and_then(Path::parent)
        .unwrap_or(&placed.path);
    let pid = RemoteAgentPid::File(worktree.join(remote_agent_pid_file(session_id)));
    record_remote_owner_from(placed, pointer, &pid).await
}

/// [`record_remote_owner`], reading the agent's pid from `pid`: the owner record a launch makes.
pub(crate) async fn record_remote_owner_from(
    placed: &PlacedSessionPointer,
    pointer: &SessionPointer,
    pid: &RemoteAgentPid,
) -> Result<SessionPointerOwner, Status> {
    let (shell, alias): (&dyn RemoteShell, String) = match &placed.host {
        PointerHost::Ssh { shell } => (shell, shell.alias().to_string()),
        PointerHost::Remote { shell } => (shell.as_ref(), shell.address().to_string()),
        PointerHost::Local => {
            return Err(Status::failed_precondition(format!(
                "the session pointer {} is on this host: the daemon that placed it owns it",
                placed.path.display()
            )));
        }
    };
    // The pointer, without an owner, as compact JSON: the target appends the owner it read.
    let mut unowned = pointer.clone();
    unowned.owner = None;
    let body = serde_json::to_vec(&unowned)
        .map_err(|e| Status::internal(format!("the session pointer cannot be encoded: {e}")))?;
    let remote = format!(
        "set -e; umask 077; \
         {read_pid} host=$(hostname); \
         case \"$pid\" in ''|*[!0-9]*) echo \"the agent's pid file holds '$pid'\" >&2; exit 1;; esac; \
         case \"$host\" in ''|*[!A-Za-z0-9._-]*) echo \"the host name '$host' is not a plain name\" >&2; exit 1;; esac; \
         body=$(cat); \
         printf '%s,\"owner\":{{\"pid\":%s,\"host\":\"%s\"}}}}' \"${{body%?}}\" \"$pid\" \"$host\" > {file}.owned; \
         mv {file}.owned {file}; \
         echo \"$pid $host\"",
        read_pid = pid.consume_snippet(),
        file = shell_single_quote(&placed.path.to_string_lossy()),
    );
    let output = shell
        .exec(ExecRequest::new(remote).with_stdin(body))
        .await
        .map_err(|error| remote_status(&error))?
        .success_or_exited()
        .map_err(|error| remote_status(&error))?;
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
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
/// directory) gets its pointer with nothing to exclude it from — the session's hooks directory
/// likewise (`session_hooks_worktree`). Refused when git cannot be run.
pub(crate) fn inside_a_git_checkout(worktree: &Path) -> Result<bool, Status> {
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
                "git could not be run to keep the session's files out of commits: {e}"
            ))
        })
}

/// Remove `worktree`'s pointer on `host`, if any: the session is gone.
pub(crate) async fn remove_session_pointer(fs: &dyn tddy_fs::Fs, worktree: &Path, host: &PointerHost) {
    remove_placed_pointer(fs, &PlacedSessionPointer {
        path: session_pointer_path(worktree),
        host: host.clone(),
    })
    .await;
}

/// Remove `placed`, on the host it is on.
pub(crate) async fn remove_placed_pointer(fs: &dyn tddy_fs::Fs, placed: &PlacedSessionPointer) {
    let (shell, alias): (&dyn RemoteShell, String) = match &placed.host {
        PointerHost::Local => {
            remove_pointer_file(fs, &placed.path);
            return;
        }
        PointerHost::Ssh { shell } => (shell, shell.alias().to_string()),
        PointerHost::Remote { shell } => (shell.as_ref(), shell.address().to_string()),
    };
    let removed = shell
        .exec(ExecRequest::new(format!(
            "rm -f {}",
            shell_single_quote(&placed.path.to_string_lossy())
        )))
        .await;
    match removed {
        Err(e) => {
            log::warn!(
                "could not remove the session pointer {} on {alias}: {e}",
                placed.path.display()
            );
        }
        Ok(output) if output.status != 0 => {
            log::warn!(
                "could not remove the session pointer {} on {alias}: the command exited with status {}: {}",
                placed.path.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(_) => {}
    }
}

pub(crate) fn remove_pointer_file(fs: &dyn tddy_fs::Fs, path: &Path) {
    if let Err(e) = fs.remove_file(path) {
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
    fs: std::sync::Arc<dyn tddy_fs::Fs>,
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
            remove_placed_pointer(&*fs, &pointer).await;
            if let Some(on_exit) = on_exit {
                on_exit(capture).await;
            }
        })
    }))
}

#[cfg(test)]
mod cli_session_pointer_unit_tests;
