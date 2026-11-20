# 2026-11-20 — Tabs are recorded in a roster, an agent tab can run in tmux, and live tmux tabs re-attach at startup

**Type:** Feature · **Stack:** `#session-liveness` 2/4 (PR #959) · **Packages:** `tddy-changeset`,
`tddy-cli-sessions`, `tddy-terminal-rpc`, `tddy-session-lifecycle`, `tddy-agent-launch`, `tddy-daemon`,
`tddy-session-web`, `tddy-coder` · Feature: [session-liveness.md](../ft/daemon/session-liveness.md) ·
Backlog: `2026-11-03-detached-tmux-agent-output-is-not-recorded` (extended to tabs, not closed),
`2026-11-02-tmux-listing-and-tab-use-two-privilege-drops` (not touched — the new paths use the two the
tmux tab already uses)

## Problem

Nothing recorded which tabs a session had or how each was launched, only the main could run in tmux,
and a tmux agent that outlived its daemon stayed inactive until someone opened its terminal.

## What changed

- **`tddy_changeset::tab_roster`**: `TabRoster` / `TabEntry` / `LaunchNode` and `read_tab_roster` /
  `write_tab_roster` for `<session dir>/tabs.yaml` — a launch chain per tab, outermost first, unknown
  kinds and keys kept verbatim. A separate file because `.session.yaml` is `deny_unknown_fields` and is
  rewritten by unlocked read-modify-writes.
- **`TabRosterStore`** (`tddy-cli-sessions`, held by `CliSessionManager`): the one writer, `record` and
  `forget` under one lock per session directory. `CliSessionManager::start_terminal_under_id` starts a
  shell or agent under a recorded id (what Resume will call).
- **Per-terminal tmux**: `attach_terminal_to_tmux` / `reattach_terminal`; the detach watcher registry and
  the settling hold are keyed by `(session, terminal)`; an ended tmux client of an agent is classified for
  a tab as for a main, and only a main's end drops session state or announces an agent end. A tab attached
  to the user's own tmux session (kind `tmux`) is a client of nothing the session keeps. The wrapped tab's
  tmux session is `tddy-<session id>-<terminal id>` (`agent_tab_tmux_session_name`).
- **Proto**: `LaunchNode`, `StartTerminalSessionRequest.wrappers` (6), `TerminalSessionInfo.launch` (5);
  `TerminalKind::TmuxWrappedAgent`, `from_wrapped_request`, `TMUX_WRAPPER_KIND`; `TerminalRoster::launch_chains`
  (a defaulted trait method — the descriptor carries no launch field).
- **Roster behaviour** (`recorded_tabs.rs`): every started tab is recorded; a tab that cannot be recorded
  is stopped and its start fails; closing a tab removes it, and closing a tmux-wrapped tab first ends its
  tmux session; a tab that exits on its own stays. The main's tmux run is recorded as `[tmux, agent]`
  (`tmux_main_roster`); `reattach_recorded_main` reads the roster, falling back to `.session.yaml`.
- **Startup re-attach**: `DaemonSessionHost::reattach_live_tmux_tabs` — a daemon startup task re-attaches
  the main and every wrapped tab whose tmux session is alive as the OS user its `tmux` node records; a
  session with no roster falls back to its `.session.yaml` main, the OS user being the configured one whose
  tmux holds it. It reports what it re-attached and what it left, and why.
- **Web**: **Add agent ▸ in tmux** and a tmux marker on the tab.

## Wrap cleanups

- `terminal-rpc` `service.rs` 577 → 467 production lines (engine-driven `move_item` /
  `move_impl_members`): the entry and its coordinate in `service/entry.rs`; the control-lease relay, its
  capacity and the two control helpers in `service/control_relay.rs`.
- Functions this PR grew past 60 lines, shortened: `start_tab` (80 → 26, one method per kind, the longest 22),
  `DetachWatch::start` (63 → 17 + `watch_until_gone` 28 + two helpers), `start_harness_in_tmux` (67 → 51,
  attach and record in `attach_and_record_main`, 35), `RuntimeTasks::spawn` (140 → 25, one function per task),
  the re-attach of a tab (61 → 49).
- The seven `StartTerminalSession` request literals in the coder's participant tests share one builder.
- A test pins that closing a tmux-wrapped tab ends its tmux session and leaves the roster.

## Code issues

- **Deleted:** `tddy-terminal-rpc` `oversized-file-service` — final measurement: `service.rs` 573 → 467
  production lines by the gate's counter (546 at its last measurement; 577 with this PR's growth).
- **Rows added, still open:** `tddy-session-lifecycle` `oversized-file-connection-service` (`restructure
  lines` 434 → 437: a `mod` and a `pub use`); `tddy-cli-sessions` `complexity-pty-spawn-spawn-tool-on`
  (`spawn_tool_on` untouched at 77 lines).
- The changeset's third item, `complexity-claude-cli-tmux-reattach-recorded-main`, was deleted by 1/4.
- **Not measured:** `tddy-tools analyze coverage` did not run (instrumented builds of this workspace on a
  disk shared with other builds), so CRAP scores were not re-derived; function length was measured against
  the merge base instead.

## Tests

`tddy-session-lifecycle`: `tab_roster_acceptance` (3), `tmux_wrapped_tab_acceptance` (7), and the
re-attach, harness-tmux, tmux-tab and liveness suites it shares code with; unit: `tab_roster` (round trip,
unknown content, absent file), `TabRosterStore` (32 concurrent records lose none, `forget` removes only its
id), `start_terminal_under_id` (4), `agent_tab_tmux_session_name` (2), `TerminalKind::from_wrapped_request`
(4). Web: `NewTerminalMenuTmuxAcceptance` (1) and the `SessionTerminalTabs*` specs (34).

## Left as it is

- The startup task has no daemon-level test; `reattach_live_tmux_tabs` is driven directly by the
  acceptance suite.
- Only a `tmux-attach` tab is exercised for the user's own tmux; `tmux-new` is recorded as `tmux-attach`.
- A pre-roster main's OS user is probed across the configured users.
