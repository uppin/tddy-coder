# 2026-11-20 — Resume restores every tab of a session under its recorded id

**Type:** Feature · **Stack:** `#session-liveness` 4/4 (PR #961) · **Packages:** `tddy-agent-launch`,
`tddy-service`, `tddy-session-lifecycle`, `tddy-session-web`, `tddy-client-web` (generated),
`tddy-rust-typescript-tests` (generated) · Feature: [session-liveness.md](../ft/daemon/session-liveness.md)

## Problem

Resume restored the main agent only. A second Claude tab or a tmux agent tab was lost at every stop, and
an agent tab recorded no conversation, so even a restarted one could not have continued it.

## What changed

- **`tddy_agent_launch::restore_tabs`** (`opencode_cli_spawn/restore_tabs/`): `restore_plan` (pure, per
  roster entry over a `LiveTabs` snapshot: `LeaveLive`, `ResumeAgent`, `FreshAgent`, `ReattachTmux`,
  `RelaunchTmux`, `FreshShell`, `ReattachTmuxAttach`, `Drop`) and `restore_tab_roster` (carries it out under
  each recorded id through `start_terminal_under_id` / `reattach_terminal`, forgets a dropped tab).
  `new_agent_tab`, `agent_tab_argv`, `agent_launch_node` and `tab_conversation_is_resumable` are shared by
  a tab's start and its restore.
- **Every resume path** (host, sandboxed, target) calls `LaunchSessions::restore_roster_on_resume` after
  main and fills `ResumeSessionResponse.restored_tabs`.
- **Proto:** `ResumeSessionResponse.restored_tabs = 5`, `RestoredTab { terminal_id, outcome, reason }`,
  `RestoredTabOutcome { RESTORED, FRESH, DROPPED }`; TS regenerated in `tddy-client-web` and
  `tddy-rust-typescript-tests`.
- **Agent tabs** record `conversation_id` and `resumable` (`"false"` for opencode, muse, warp); a cursor
  tab is started with a chat the agent creates.
- **Web:** the strip keeps the active tab across Resume (`lastActiveTab`, a per-session memory in
  `useSessionTerminals`) and `useSessionAttachment.resumeNotice` feeds `SessionsDrawerResumeNotice`.
- `tddy-agent-launch` now depends on `tddy-terminal-rpc` (`login_shell_for`, `TMUX_KIND`); no dependency
  cycle (`tddy-terminal-rpc` does not depend on it).

## Wrap cleanups

- `restore_tabs.rs` 639 -> a root re-exporting `plan.rs` (217), `roster.rs` (376), `new_tab.rs` (104) and
  `on_resume.rs` (31); `carry_out` (85 -> 53) split into `start_agent`, `start_under_id`, `reattach` and
  `relaunch_tmux_tab`. The moves were done mechanically, not through the restructure engine.
- `resume.rs` 475 -> 490 production lines (505 before the cleanup): the target shell is resolved by
  `resumed_target_shell`. `resume_cli_harness` 86 -> 83, `resume_sandboxed_cli_harness` 86 -> 77
  (`recorded_managed_recipe`), `resume_session_at_session_coordinate` 190 -> 189; each keeps its open
  record, with a row.

## Known gaps

- Sandbox tab restore (a jail-side tab spawn plus a `sandbox` wrapper node) and target tab restore (a
  target tab launch plus a target wrapper node) are separate stack nodes; until then only host-side tabs
  are restored, and a chain holding another kind is dropped with a reason.
- opencode, muse and warp tabs reopen fresh (`TODO(opencode-cli)`, `TODO(muse-cli)`, `TODO(warp-cli)`).
- The sandboxed resume path has no test of its own: its jail harness is the macOS-only one that already
  fails on `master`.
- A target session's host-recorded shell tab is probably dropped on resume.
