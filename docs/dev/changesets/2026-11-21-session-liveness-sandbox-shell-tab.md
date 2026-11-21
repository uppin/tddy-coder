# 2026-11-21 — A sandboxed session's shell tab runs in a jail of its own

**Type:** Feature · **Stack:** `#session-liveness` 5/8 (PR #964) · **Packages:** `tddy-terminal-rpc`,
`tddy-changeset`, `tddy-daemon-sandbox`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-session-activity`,
`tddy-service`, `tddy-coder`, `tddy-session-web`, `tddy-client-web` (generated), `tddy-rust-typescript-tests`
(generated) · Feature: [session-liveness.md](../ft/daemon/session-liveness.md#a-sandboxed-sessions-tabs)

## Problem

A sandboxed session's secondary tab was an unjailed host shell, and it could not even be streamed while the
main jail was registered: the terminal store answered only `main` for a sandboxed session. The jail's point,
confinement of what runs against the worktree, stopped at the first extra tab. Daemon shutdown also left every
session's main jail running.

## What changed

- **`tddy_daemon_sandbox::tab_jails`** — `TabJails`, the registry of tab jails keyed `(session_id,
  terminal_id)`: `start` (one jail per tab, `ALREADY_EXISTS` otherwise, a refused spawn registers nothing),
  `get`, `stop`, `stop_session`, `stop_all`, `live`, and a death watch (`subscribe_ended`) that forgets a jail
  whose runner exited. `TabJailSpawn` is the port that starts one; `TabJailLaunch` carries the session, tab,
  directory, worktree, the tab's `TabJailPaths`, and **who the jail runs for** (`os_user`, `login_shell`, and the
  owner's `subject`, the GitHub user their session token resolves to). `tab_jail_paths.rs`: `<session dir>/sandbox/tabs/<terminal_id>/`
  with an IPC socket hashed from both ids and its own marker, exec plan, profile, egress dir and scratch;
  a terminal id that cannot name a directory is `INVALID_ARGUMENT`.
- **`SandboxSessionManager::stop_all`** is the only growth of `sandbox_session.rs` (`launch_jailed` and
  `dial_and_bridge` are untouched).
- **`tddy_agent_launch::tab_jail_launch`** — `impl TabJailSpawn for LaunchSessions`: the session's grants are
  lowered as its main jail's are (`lower_tool_jail`, `build_tool_jail_plan`, `spawn_sandbox_plan`, the host
  relay) **on the session's provider route**, re-resolved from the owner's vault by `subject` (`LaunchHost::subject_account_access`,
  `SessionAccountAccess::for_subject`) as a resume's is (`tab_route`: a removed account or locked vault refuses
  the tab); the worktree is mounted writable as the tab's cwd; the
  runner execs `launch.login_shell`. The route's env and credentials are not given to the shell.
  The pieces `launch_jailed` shares (`jail_path`, `write_exec_plan`, `RunnerArgv`, `runner_argv`, `host_home`,
  `JAIL_READY_TIMEOUT`) are `pub(crate)`.
- **Records are the main's alone.** A tab jail writes no usage context and no capture recording (a code
  comment and the feature doc say so; no code change).
- **Terminal wire** — `sandbox` is accepted as the only wrapper around the login shell
  (`TerminalKind::SandboxedShell`, `SANDBOX_WRAPPER_KIND`); around an agent, nested in `tmux`, or on an
  unsandboxed session it is refused. `TerminalLaunch` gains `subject` (from `github_user(&req.session_token)`) so a tab that
  acts for the owner can resolve their vault; no session token is carried past the request. The participant's `CoderTerminalRoster` refuses `sandbox`.
- **Roster** — `tabs.yaml` records `[sandbox, shell]` (`SANDBOX_KIND`, `LaunchNode::sandbox()`); a jail that
  cannot start fails the start and records nothing. `restore_plan` plans `[sandbox, shell]` as a shell in a new
  jail (`TabRestore::JailedShell`) and drops `sandbox` around an agent, a tmux node or nothing, naming why;
  `RosterRestore` gains `subject` (from `user_resolver(&req.session_token)`), `tab_jails` and `tab_jail_spawn`, and the restored launch carries the
  resume's OS user and that user's login shell.
- **Store and teardown** — `DaemonTerminalSessionStore` resolves any non-main id of a sandboxed session by its
  owner (a tab jail, or the terminal manager for an "On host" tab). Closing a tab, deleting, stopping and
  resuming a session stop the session's tab jails; `shut_down_children` stops every tab jail **and every
  main jail**.
- **Proto / web** — `SessionEntry.sandbox = 41` (read from `.session.yaml` by `tddy-session-activity`);
  `NewTerminalMenu` offers "In sandbox" (default) and "On host" for a sandboxed session only. TS regenerated.

## Wrap cleanups

- `run_tab_jail` (120 lines at the green) split into `plan_tab_jail`, `tab_runner_spawn`,
  `bridge_tab_terminal` and the spawn/wait; `spawn_shell` (62) lost `session_agent_of_tab`. No function of
  `tab_jail_launch.rs` is over 60 lines.
- Functions this PR grew in files successor nodes also modify were brought **below their length at the merge
  base**, not split: `resume_sandboxed_cli_harness` 77 -> 72 (`stop_previous_run`),
  `DaemonSessionHost::new` 101 -> 97 (`default_session_agent_rosters`), `session_entry_from_listing`
  122 -> 52 (`session_entry_of_listing`, `orchestration_mark_proto`). The first two keep their open records,
  with a row.
- `sandbox_session.rs` 924 -> 938 production lines (`stop_all`, the planned growth); its record keeps a row.

## Verification

Scoped to the touched packages: the tab-jail and restore unit tests, the `sandbox_shell_tab` /
`resume_tab_roster` / `sandbox_shutdown` acceptance suites against an in-memory `TabJailSpawn` over real
processes, `tddy-terminal-rpc`'s wrapper tests, and the Cypress menu spec. Whole-workspace health is CI's.

`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md` is narrowed, not deleted: §1
(shutdown) is resolved and pinned by `shutting_down_the_daemon_stops_main_and_tab_jails` (before: `shut_down_children`
stopped the CLI sessions and the workspace jails and no sandboxed session's runner; after: it stops every main and
every tab jail); §2 (a death watch) is closed for tab jails and open for the main's.

## Known gaps

- **The kernel sandbox is not tested by CI.** `sandbox_shell_tab_jail_linux` self-skips off Linux and needs a
  VM; it waits for VM-backed testing ([`docs/dev/todo/2026-11-02-ci-runs-no-linux-jail.md`](../todo/2026-11-02-ci-runs-no-linux-jail.md)).
- A tab jail, like the main's, is spawned by the daemon under its own identity: `os_user` selects the login
  shell and is logged, but nothing switches uid for either kind of jail.
- The main jail's death is still unwatched (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`, §2).
- A jail terminal cannot be resized and has no control lease (the main's limits, by decision).
- Agent tabs in a jail (`sandbox-agent-tab`) and tabs on a target (`target-shell-tab`, `target-agent-tab`) are
  later stack nodes.
