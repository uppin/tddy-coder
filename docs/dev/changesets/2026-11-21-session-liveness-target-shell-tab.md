# 2026-11-21 — A target session's shell tab runs on the target

**Type:** Feature · **Stack:** `#session-liveness` 6/8 (PR #965) · **Packages:** `tddy-terminal-rpc`,
`tddy-changeset`, `tddy-cli-sessions`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-coder`,
`tddy-session-web`, `tddy-client-web` (generated), `tddy-testing-commons` (test support) · Feature:
[session-liveness.md](../ft/daemon/session-liveness.md#a-target-sessions-tabs)

## Problem

A target (ssh or Coder) session's secondary tab was a host shell in `recorded_worktree`, a path that exists only
on the target, so it could not start for a stopped session and Resume dropped or misplaced it. The only launch
that reaches a target, `launch_on_target`, is the main's: it writes the session's pid file, rewrites the session
pointer and stops the whole session when the agent does not start. None of that is a tab's to do.

## What changed

- **`tddy_agent_launch::remote_agent_launch::target_tab`** — `launch_tab_on_target(manager, session_id,
  terminal_id, TargetTab { shell, worktree, session_dir })` and `target_shell_tab_command(worktree)`: `cd '<wt>' &&
  exec "${SHELL:-/bin/sh}" -l`, the target user's own login shell with the login's own environment (not
  `target_login_exec`'s `env -i`). The target is reached first, with `cd '<wt>'` through the port, so an
  unreachable target or a missing worktree fails the tab with the transport's words before any terminal is
  registered. The tab writes no pid file, places or rewrites no session pointer, applies no startup window and
  never stops the session. `client_command` shares its body with `transport_client`; `launch_on_target` is
  unchanged.
- **`tddy_cli_sessions::CliSessionManager::spawn_rendered_client_tab`** (+ `ClientTab`) — the non-main sibling of
  `spawn_rendered_client_main`: registers the transport's client under the tab's id (refused for `main` and for a
  live id), through `spawn_tool_on`'s existing parameters.
- **`tddy_agent_launch::tab_target_shell`** — the `TabTargetShell` port (`target_of(session_dir, subject)`) and the
  daemon's `SessionTargetShells`: the session's `.session.yaml` `target` and `repo_path`, resolved as a start
  resolves it (`shell_for_session_target`) with the **caller's** subject and the owner's vault. A session with no
  target or no worktree, a locked vault, and an account the vault does not hold are `FAILED_PRECONDITION`.
  `LaunchSessions::tab_target_shell()` returns a test's double or the daemon's.
- **Terminal wire** — `target` is accepted as the only wrapper around the login shell (`TerminalKind::TargetShell`,
  `TARGET_WRAPPER_KIND`); around an agent, or nested with another wrapper, it is `INVALID_ARGUMENT`. `TerminalLaunch`
  carries `subject` (set in `start_terminal_session`, where the token is already resolved). The participant's
  `CoderTerminalRoster` refuses it. The `LaunchNode` comment in the proto names the `target` kind.
- **Roster** — `tabs.yaml` records `[target, shell]` (`TARGET_KIND`, `LaunchNode::target()`, which records no
  properties: the transport and worktree are the session's, read on every start). `restore_plan` plans it as
  `TabRestore::TargetShell` and drops `target` around anything else, naming why; `RosterRestore` and
  `restore_roster_on_resume` carry `subject` and the target-shell port, so Resume reopens the tab on the target
  or drops it with the reason (an unreachable target, a locked vault).
- **Host tab of a target session** — a tab "On this host" runs in the session's directory on this host (where
  its main's client runs): its recorded worktree is on the target.
- **Web** — `NewTerminalMenu` offers "On target" (default, `wrappers: [target]`) and "On this host" for a session
  whose `SessionEntry.target` is set, unless it is sandboxed or a tddy-coder participant (the two features never
  meet in one session).
- **Test support** — the stand-in ssh machine's login shell answers `-l` and records where it started
  (`tddy_testing_commons::remote_target::StandInSshTarget::login_shell_started_in`, written through a `.part` file
  and `mv`, so a reader never sees a half-written record); the stand-in Coder workspace moved out of
  `coder_agent_launch_acceptance.rs` into `tests/coder_workspace_support`, shared by both suites.

## Wrap

- No function this PR grew is over 60 lines; no source file crossed or grew past 500 production lines (the
  largest, `svc_host_builders.rs`, is 469).
- Code issues re-measured: `launch_on_target` 43 lines (untouched, location moved 227 → 235 by the new `mod`
  and re-exports) and `spawn_tool_on` 13 parameters (untouched) each carry a row. `NewTerminalMenu.tsx` repeats
  the sandbox group's markup for the target group; a successor edits both, so the dedupe waits for the end of
  the stack (see `docs/dev/todo/2026-11-20-session-liveness-oversized-files-after-the-stack.md`).
- Fixture todo `2026-11-16-remote-shell-test-fixtures-are-duplicated.md` is narrowed: the agent-launch suites now
  share one stand-in Coder workspace.

## Decisions

- **A target that resolves but cannot be reached is `UNAVAILABLE`** carrying the transport's message (developer,
  2026-11-21): the failure is the target's, not the request's. `launch_tab_on_target`'s `RemoteLaunchError::Shell`
  maps to it in `start_target_shell_tab`; a target that cannot be *resolved* (a locked vault, a missing account,
  no recorded target or worktree) stays `FAILED_PRECONDITION`. Both are pinned in
  `tddy-session-lifecycle`'s `target_shell_tab_acceptance`.

## Verification

Scoped to the touched packages: `./test -p tddy-terminal-rpc -p tddy-changeset -p tddy-cli-sessions -p
tddy-testing-commons -p tddy-agent-launch -p tddy-session-lifecycle`, `cargo clippy --all-targets -- -D warnings`
over those and `tddy-coder`, and the single Cypress spec `NewTerminalMenuTargetAcceptance`. Whole-workspace
health is CI's.

## Known gaps

- An agent on a target (`target-agent-tab`, 8/8) and an agent in a jail (`sandbox-agent-tab`, 7/8) are later
  stack nodes: `target` around an agent is refused here.
