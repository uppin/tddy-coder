//! A target session's **tmux tabs on the target** (`#target-tmux` 2/2): the target login's tmux
//! sessions listed through its login shell, and a tab that is a client of a tmux session there — a
//! tmux session a person keeps, one the tab names anew, or one the tab's agent runs in.
//!
//! Every tab runs one command through the target's login shell, in the session's worktree there:
//!
//! ```text
//! cd '<worktree>' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux -T clipboard new-session -A -s '\''<name>'\'' [<inner>]'
//! ```
//!
//! `new-session -A` attaches to `<name>` when it lives and makes it otherwise, so a start, a re-attach
//! and a recreate are the same command. The client announces the clipboard feature, as a host tab's
//! does, so the target's tmux forwards clipboard writes to it.
//!
//! An agent's own session ([`target_agent_tmux_tab_command`]) additionally enables that forwarding
//! before it attaches — tmux's defaults swallow a program's OSC 52 — while a person's own session
//! keeps the target login's tmux configuration untouched.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-23-target-tmux.md (n2)

use std::path::Path;
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_cli_sessions::tmux::{parse_list_sessions, LIST_SESSIONS_FORMAT};
use tddy_core::ssh_exec::shell_single_quote;
use tddy_remote_shell::{ExecRequest, RemoteShell, RemoteShellError};
use tddy_terminal_rpc::TmuxSession;

use super::agent_probe::probe_tmux_on_target;
use super::target_tab::{reach_the_worktree, spawn_tab_client};
use super::{target_login_run, RemoteLaunchError, TargetTab};

/// The kind a tmux tab on a target is listed under.
const TMUX_TAB_KIND: &str = "tmux";

/// What tmux says when no server runs for the login: an empty listing, not a failure.
fn no_tmux_server(stderr: &str) -> bool {
    let lowered = stderr.to_lowercase();
    lowered.contains("no server running") || lowered.contains("error connecting")
}

/// `inner` run by the target's login shell in `worktree`.
fn in_the_worktree(worktree: &Path, inner: &str) -> String {
    format!(
        "cd {} && {}",
        shell_single_quote(&worktree.to_string_lossy()),
        target_login_run(inner)
    )
}

/// The tmux sessions of the target login `shell` reaches, listed through its login shell from
/// `worktree`: empty when no tmux server runs there, [`RemoteLaunchError::TmuxNotOnTarget`] when its
/// login shell finds no `tmux`.
pub async fn list_target_tmux_sessions(
    shell: &dyn RemoteShell,
    worktree: &Path,
) -> Result<Vec<TmuxSession>, RemoteLaunchError> {
    probe_tmux_on_target(shell, worktree).await?;
    let listing = in_the_worktree(
        worktree,
        &format!(
            "exec tmux list-sessions -F {}",
            shell_single_quote(LIST_SESSIONS_FORMAT)
        ),
    );
    let output = shell
        .exec(ExecRequest::new(listing))
        .await
        .map_err(RemoteLaunchError::Shell)?;
    if output.status == 0 {
        return Ok(parse_list_sessions(&String::from_utf8_lossy(
            &output.stdout,
        )));
    }
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if no_tmux_server(&stderr) {
        return Ok(Vec::new());
    }
    Err(RemoteLaunchError::Shell(RemoteShellError::Exited {
        status: output.status,
        stderr,
    }))
}

/// The command a tmux tab runs on a target: `tmux new-session -A -s <name>` in `worktree`, through
/// the target's login shell, running `inner` (an already-quoted shell command) as the new session's
/// command when it is made, or the login's shell when `inner` is `None`. The client announces the
/// clipboard feature (`-T clipboard`), as a host tab's does.
pub fn target_tmux_tab_command(worktree: &Path, name: &str, inner: Option<&str>) -> String {
    let mut tmux = format!(
        "exec tmux -T clipboard new-session -A -s {}",
        shell_single_quote(name)
    );
    if let Some(inner) = inner {
        tmux.push(' ');
        tmux.push_str(&shell_single_quote(inner));
    }
    in_the_worktree(worktree, &tmux)
}

/// The shell fragment that enables clipboard forwarding on the target's tmux session `name` (an
/// agent's own): `allow-passthrough` on the session's window, so tmux-aware programs' wrapped
/// OSC 52 writes pass through, and the server's `set-clipboard` upgraded from the default
/// `external` to `on`, so plain writes are forwarded. An explicit `off` is the target login's
/// hardening and is left alone — the same rule [`tddy_cli_sessions::tmux`] applies on the host
/// ([`set_clipboard_needs_enabling`](tddy_cli_sessions::tmux::set_clipboard_needs_enabling)), in
/// the target's shell because no tddy binary runs there. The steps never abort the chain (`;`, and
/// each silenced): an old tmux without `allow-passthrough` still forwards plain writes, and a
/// session already gone fails the caller instead.
fn target_tmux_clipboard_fragment(name: &str) -> String {
    let window = shell_single_quote(&format!("={name}:"));
    format!(
        "tmux set-option -t {window} allow-passthrough on 2>/dev/null; \
         if [ \"$(tmux show-options -s set-clipboard 2>/dev/null)\" = \"set-clipboard external\" ]; \
         then tmux set-option -s set-clipboard on 2>/dev/null; fi"
    )
}

/// The command an agent tab runs in tmux on a target: the session `name` created detached running
/// `inner` (an already-quoted shell command), clipboard forwarding enabled on it, then the tab
/// attached to it — in `worktree`, through the target's login shell. A session already living is
/// not created twice: the duplicate-session complaint is silenced and the tab attaches to it, with
/// forwarding enabled on it all the same. The client's `-T clipboard` and the enablement are what
/// carry a program's OSC 52 copy to the viewer.
pub fn target_agent_tmux_tab_command(worktree: &Path, name: &str, inner: &str) -> String {
    let tmux = format!(
        "tmux new-session -d -s {} {} 2>/dev/null; {}; exec tmux -T clipboard attach-session -t {}",
        shell_single_quote(name),
        shell_single_quote(inner),
        target_tmux_clipboard_fragment(name),
        shell_single_quote(&format!("={name}")),
    );
    in_the_worktree(worktree, &tmux)
}

/// Enable clipboard forwarding on the target's tmux session `name` (an agent's own), through its
/// login shell: [`target_tmux_clipboard_fragment`]. The session must already live — the caller runs
/// this once the agent is known to have started. A target whose tmux refuses is the shell's error.
pub async fn enable_target_tmux_clipboard(
    shell: &dyn RemoteShell,
    name: &str,
) -> Result<(), RemoteLaunchError> {
    shell
        .exec(ExecRequest::new(target_login_run(
            &target_tmux_clipboard_fragment(name),
        )))
        .await
        .map_err(RemoteLaunchError::Shell)?
        .success_or_exited()
        .map_err(RemoteLaunchError::Shell)?;
    Ok(())
}

/// [`enable_target_tmux_clipboard`], for a caller whose agent already runs: a session that refuses
/// still runs its agent, so a failure is logged — copies from its programs may not reach the
/// viewer — rather than returned.
pub async fn enable_target_tmux_clipboard_logged(shell: &dyn RemoteShell, name: &str) {
    if let Err(error) = enable_target_tmux_clipboard(shell, name).await {
        log::warn!(
            "copies from programs in tmux session {name:?} on the target may not reach the viewer: {error}"
        );
    }
}

/// Open the tab `terminal_id` of `session_id` as a client of the tmux session `name` on the target
/// `tab.shell` reaches — attached when it lives, made in `tab.worktree` otherwise — and register the
/// local PTY that is its terminal with `manager`. The target is reached first and asked for tmux:
/// an unreachable target fails [`RemoteLaunchError::Shell`], a target without tmux
/// [`RemoteLaunchError::TmuxNotOnTarget`], before any terminal is registered.
pub async fn launch_tmux_tab_on_target(
    manager: &CliSessionManager,
    session_id: &str,
    terminal_id: &str,
    tab: TargetTab<'_>,
    name: &str,
) -> Result<Arc<PtyHandle>, RemoteLaunchError> {
    reach_the_worktree(&tab).await?;
    probe_tmux_on_target(tab.shell.as_ref(), tab.worktree).await?;
    let command = target_tmux_tab_command(tab.worktree, name, None);
    spawn_tab_client(
        manager,
        session_id,
        terminal_id,
        &tab,
        TMUX_TAB_KIND,
        &command,
    )
    .await
}

/// Kill the tmux session `name` on the target `shell` reaches, through its login shell. A tmux
/// session — or server — already gone is not an error.
pub async fn end_target_tmux_session_named(
    shell: &dyn RemoteShell,
    name: &str,
) -> Result<(), RemoteLaunchError> {
    let name = shell_single_quote(&format!("={name}"));
    // A session or server already gone is what the end wants; any other refusal is reported.
    let inner = format!(
        "out=$(tmux kill-session -t {name} 2>&1) && exit 0; \
         case \"$out\" in *\"can't find session\"*|*\"no server running\"*|*\"error connecting to\"*) exit 0;; esac; \
         printf '%s\\n' \"$out\" >&2; exit 1"
    );
    shell
        .exec(ExecRequest::new(target_login_run(&inner)))
        .await
        .map_err(RemoteLaunchError::Shell)?
        .success_or_exited()
        .map_err(RemoteLaunchError::Shell)?;
    Ok(())
}

/// Whether the target `shell` reaches holds the tmux session `name`.
pub async fn target_tmux_session_lives(
    shell: &dyn RemoteShell,
    name: &str,
) -> Result<bool, RemoteLaunchError> {
    let inner = format!(
        "exec tmux has-session -t {}",
        shell_single_quote(&format!("={name}"))
    );
    let output = shell
        .exec(ExecRequest::new(target_login_run(&inner)))
        .await
        .map_err(RemoteLaunchError::Shell)?;
    Ok(output.status == 0)
}

#[cfg(test)]
#[path = "target_tmux_tab_tests.rs"]
mod tests;
