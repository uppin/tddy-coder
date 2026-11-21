# 2026-11-21 — A target session's agent tab runs the agent on the target

**Type:** Feature · **Stack:** `#session-liveness` 8/8 (PR #967) · **Packages:** `tddy-agent-launch`,
`tddy-terminal-rpc`, `tddy-session-lifecycle`, `tddy-session-web`, `tddy-coder`, `tddy-client-web` (generated) ·
Feature: [session-liveness.md](../ft/daemon/session-liveness.md#a-target-sessions-tabs)

## Problem

An agent tab of a target (ssh or Coder) session resolved the agent binary on the daemon's host
(`new_agent_tab` → `HostCodingAgents::resolve`) and minted its chat there, so a daemon host with no copy of the
agent refused the tab, and a conversation the agent mints (Cursor's chat) was started in the wrong place. A
`target` wrapper around an agent was refused outright.

## What changed

- **`tddy_agent_launch::remote_agent_launch::target_agent_tab`** — `launch_agent_tab_on_target(manager, session_id,
  terminal_id, TargetTab, &TargetAgentTab { kind, argv, env })`: reaches the worktree (`reach_the_worktree`, shared
  with the shell tab), writes any session-directory file the argv names into the worktree's `.tddy/` on the target
  and rewrites its argument (`write_rendered_session_files`, now over a `SessionFilesPlace` the main's launch and
  the tab both build), and runs `cd '<wt>' && <target_login_exec(argv, env)>` as a non-main client tab
  (`spawn_tab_client`, shared with the shell tab). No pid file, no pointer, no startup window; its failure ends
  only the tab. `launch_tab_on_target` keeps its behaviour; `launch_on_target` is unchanged apart from the place it
  passes.
- **`restore_tabs::new_target_tab`** — `target_agent_tab` (the spec's **bare command** as `{binary}`, `on_target`
  rendered, the `pass_env` values the daemon holds and `run.env` as the carried environment; nothing resolved on
  this host) and `new_target_agent_tab` (the conversation minted on the target through
  `ConversationStart { target: Some(..) }`).
- **Roster** — `[target, agent]` is recorded; `restore_plan` plans it as `TabRestore::TargetAgent { harness,
  resume_conversation }` (resume where the harness can, fresh with the reason otherwise; a node inside the agent is
  dropped). The restore moved, with the jailed ones, into `restore_tabs/roster/wrapped_tabs.rs`.
- **Terminal wire** — `target` around an agent kind is `TerminalKind::TargetAgent { agent }`; a wrapper inside the
  target is `INVALID_ARGUMENT`. The participant's `CoderTerminalRoster` refuses it. The proto comment on
  `wrappers` names it (TS regenerated).
- **Lifecycle** — `TerminalRoster::start` routes it to `start_target_agent_tab`
  (`svc_terminal_ports/target_agent_tabs.rs`): the target resolved as for a shell tab (caller's subject), the tab
  settled, launched and recorded; a tab that cannot be recorded is stopped.
- **Web** — `NewTerminalMenu`'s "On target" group lists every agent kind, each opening with `wrappers: [target]`.
- **Tests** — three n6 tests that asserted the refusal of an agent on a target were replaced by the agent-tab
  suites; the fake target's login shell answers `-lc` with the machine's `opt/agents` first on `PATH`.

## Wrap

- `restore_tabs/roster.rs` (494 production lines at #966; 540 with this PR's arm) was split here, the top node:
  `restore_jailed_shell`, `restore_jailed_agent`, `restore_target_shell`, `restore_target_agent` moved unchanged into
  `roster/wrapped_tabs.rs` — 408 and 145 production lines. Moved by hand as a child module (the restructure engine
  needs a cold rust-analyzer index of the workspace; the move is mechanical and compile-checked). `carry_out` stays
  under 60 lines.
- `launch_on_target` (code issue) grew by the one line that builds the files' place; re-measured and recorded.
- The two copies of "reach the worktree, then spawn the client tab" were folded into `reach_the_worktree` and
  `spawn_tab_client`.

## Decisions

- **The target resolves the agent** by its login `PATH` — as the target main does since #955. An agent the target
  cannot find exits `127` in its tab; nothing probes first.
- The session-**start** half of [a start on a target requires the agent on the daemon's
  host](../dev/todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md) stays open: the todo is
  narrowed to it.

## Verification

Scoped to the touched packages; whole-workspace health is CI's. `cargo clippy --all-targets -- -D warnings` over
`tddy-agent-launch`, `tddy-terminal-rpc`, `tddy-session-lifecycle` and `tddy-coder`; `cargo test` over
`tddy-agent-launch` and `tddy-terminal-rpc` (every target); `tddy-session-lifecycle`'s `target_agent_tab_acceptance`,
`target_shell_tab_acceptance` and `resume_tab_roster_acceptance`; the single Cypress spec
`NewTerminalMenuTargetAcceptance` (3 passing). The racy `session_tool_env_acceptance` and `tool_session_local_gh`
were not run here.

## Known gaps

- Cursor's `--trust` is rendered for a target tab (`on_target`) but not pinned by a test beyond the argv's first words.
