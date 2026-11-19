# Changeset: Tab roster and tmux-wrapped agent tabs

**Date**: 2026-11-19
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — several crates (changeset, cli-sessions, terminal-rpc proto, session-lifecycle, agent-launch, daemon runtime, session-web), a proto change, a new persisted file with a lock, per-terminal watcher concurrency, a startup task
**Stack**: `#session-liveness` 2/4 (PR #959, stack #962) — branch `feature/session-liveness/tmux-roster`, base `feature/session-liveness/agent-tab-liveness` (#958)

## Affected Packages

- **tddy-changeset**: `tab_roster` — the `tabs.yaml` model, read and write
- **tddy-cli-sessions**: per-terminal tmux attach, re-attach and detach watch; the roster store
- **tddy-terminal-rpc**: proto `LaunchNode`, `StartTerminalSessionRequest.wrappers`, `TerminalSessionInfo.launch`
- **tddy-session-lifecycle**: `CliManagerTerminalRoster` records every tab; eager re-attach on the host
- **tddy-agent-launch**: the main's tmux run recorded in the roster; `reattach_recorded_main` reads it
- **tddy-daemon**: the startup re-attach task
- **tddy-session-web**: "Add agent ▸ in tmux" and the tmux marker

## Related Feature Documentation

- PRD: [PRD-2026-11-19-tmux-wrapped-tab-roster.md](../../ft/daemon/1-WIP/PRD-2026-11-19-tmux-wrapped-tab-roster.md)
- Discovery: [2026-11-19-tmux-wrapped-tab-roster-initial-discovery.md](2026-11-19-tmux-wrapped-tab-roster-initial-discovery.md)

## Summary

Record every tab in `<session>/tabs.yaml` as a launch chain of `{kind, properties}` nodes; let an
agent tab run wrapped in tmux; re-attach every live tmux-wrapped tab when the daemon starts.

## Responsibility

- The roster model and file: `tddy_changeset::tab_roster::{TabRoster, TabEntry, LaunchNode,
  TAB_ROSTER_FILENAME, read_tab_roster, write_tab_roster}`. Unknown kinds and keys round-trip.
- The single writer: `tddy_cli_sessions::tab_roster_store::TabRosterStore` (`record`, `forget`),
  serialised by one lock per session.
- Starting a terminal under a given id, and tmux attach, re-attach and detach watch **per terminal**
  (generalising today's main-only `attach_main_to_tmux` / `reattach_main` / `DetachWatch`).
- Proto `LaunchNode`, `wrappers` and `launch`, plus the `tmux` wrapper in `StartTerminalSession`.
- Eager re-attach at startup: `DaemonSessionHost::reattach_live_tmux_tabs`, plus its runtime task.
- The web "in tmux" option.

## Boundaries

- Does not restore tabs on Resume — `feature/session-liveness/resume-roster` (4/4) reads this roster.
- Does not change the liveness rule — 1/4 owns `live_agent_sessions`. This node only makes
  per-terminal watchers visible to it.
- No `ssh` or other wrapper kinds: they are refused here, and only the shape admits them.
- No tool-session tabs — `feature/session-liveness/tool-tab` (3/4).
- Does not add a lock to `.session.yaml` writers.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` agent-tab-liveness (#958) | `LiveSessions`; `CliSessionManager::live_agent_sessions` (agent terminals plus detach-watched sessions); `list_sessions_in_dir(base, &LiveSessions)` | the acceptance tests read `is_active` after a restart and an eager re-attach; per-terminal watchers feed `live_agent_sessions` | redefine which kinds count as agents, change `LiveSessions`, or touch delete/signal |

## Draft PR contract

The first push publishes the `tab_roster` model (types, file name, read and write signatures), the
`TabRosterStore` and `CliSessionManager` per-terminal tmux signatures as `todo!()`, the proto
messages, `DaemonSessionHost::reattach_live_tmux_tabs` as `todo!()`, and the failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its restart tests read `is_active`, which is n1's behaviour
**Concurrent with:** `tool-tab` (3/4), once n1 is green
**Blocks:** `resume-roster` (4/4), which restores what this node records

Real dependency edges: `n1 → n2, n3      n2 → n4`

## Prerequisites

### ⚠ DURING — The tmux listing and the tmux tab drop privilege by two different mechanisms — [2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md](../todo/2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md)

A tmux-wrapped agent tab runs as the session's OS user through the PTY runtime (`setpriv`), and the
startup probe runs `tmux has-session` through the kernel's drop. Use the **same** two paths the tmux
tab already uses; adding a third is not allowed.

### ⚠ DURING — A detached tmux-run agent's output is not recorded — [2026-11-03-detached-tmux-agent-output-is-not-recorded.md](../todo/2026-11-03-detached-tmux-agent-output-is-not-recorded.md)

The gap now applies to tmux-wrapped tabs as well. It is not fixed here; extend the entry at wrap to
name tabs.

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-cli-sessions/docs/code-issues/complexity-pty-spawn-spawn-tool-on.md` — 77 lines,
  13 parameters. Opening a terminal id goes through `spawn_tool`'s existing parameter; add none.
- `packages/tddy-agent-launch/docs/code-issues/complexity-claude-cli-tmux-reattach-recorded-main.md`
  — 76 lines. The roster read goes in a new helper, not inline.
- `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` — put the
  re-attach in a new `connection_service/` submodule.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW — approved 2026-11-19
- [x] TDD Red — failing unit/integration tests
- [ ] Implementation (roster, per-terminal tmux, proto, startup re-attach, web)
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

- No terminal roster anywhere. `.session.yaml` holds the main's `tmux_session` only and is not locked.
- tmux: main only. `attach_main_to_tmux` hard-codes `MAIN_TERMINAL_ID`; the name is `tddy-<session_id>`;
  `DetachWatch` / `Watchers` are keyed by session; exit classification covers main only.
- Tab starts mint their id inside `start_terminal` / `start_agent_terminal` / `start_tmux_terminal`.
- Startup re-attaches nothing; re-attach is lazy through `ensure_main`.

### State B (Target)

- `tabs.yaml` beside `.session.yaml`, written through `TabRosterStore` only.
- Any terminal can be tmux-wrapped. Its tmux name is `tddy-<session_id>-<terminal_id>` (main keeps
  `tddy-<session_id>`, so existing tmux mains still match). Watchers and exit classification are
  keyed by `(session_id, terminal_id)`.
- `StartTerminalSession{wrappers: [tmux]}` with an agent kind starts the agent in its own tmux
  session as the session's OS user and attaches a client tab.
- On startup a task lists every session base, reads each roster (falling back to `.session.yaml`'s
  `tmux_session` for main), binds agent-tool sockets, and re-attaches each live `tmux` node. It
  reports what it re-attached and what it left.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/tab_roster_acceptance.rs`:
- [x] `opening_tabs_records_each_with_its_launch_chain`
- [x] `closing_a_tab_removes_it_from_the_roster`
- [x] `a_tab_that_exits_on_its_own_stays_in_the_roster`

`packages/tddy-session-lifecycle/tests/tmux_wrapped_tab_acceptance.rs` (`PrivateTmux`, serial):
- [x] `an_agent_tab_wrapped_in_tmux_runs_in_its_own_tmux_session`
- [x] `a_tmux_wrapped_tab_keeps_its_session_live_after_main_exits`
- [x] `a_detached_tmux_wrapped_tab_stays_live_until_its_tmux_session_ends`
- [x] `a_wrapper_other_than_tmux_is_refused_naming_it`
- [x] `the_daemon_reattaches_live_tmux_wrapped_tabs_at_startup`
- [x] `the_daemon_reattaches_a_pre_roster_tmux_main_at_startup`

`packages/tddy-session-web/cypress/component/NewTerminalMenuTmuxAcceptance.cy.tsx`:
- [x] `adds an agent tab in tmux and marks it` (written, not run — Cypress)

## Unit Tests

- `tddy-changeset` `tab_roster`: round-trip; an unknown node kind and an unknown property key
  survive a rewrite; a missing file reads as an empty roster.
- `tddy-cli-sessions` `TabRosterStore`: concurrent `record`s for one session lose nothing; `forget`
  removes only its id.
- `tddy-terminal-rpc` `TerminalKind::from_request`: `wrappers: [tmux]` with an agent kind maps to a
  tmux-wrapped agent; `[ssh]` is refused.

## Contract as published (red)

Names the changeset left open, as the draft contract fixed them:

- `TerminalKind::TmuxWrappedAgent { agent }` and `TerminalKind::from_wrapped_request(kind,
  tmux_session, wrappers: &[proto LaunchNode])`; `from_request` is unchanged. `TMUX_WRAPPER_KIND`.
- `CliSessionManager::attach_terminal_to_tmux(session_id, terminal_id, kind, worktree, os_user,
  tmux_session)` and `reattach_terminal(...)` (same parameters), in
  `cli_session_manager/tmux_terminal.rs`; `tmux::agent_tab_tmux_session_name(session_id, terminal_id)`.
- `DaemonSessionHost::reattach_live_tmux_tabs() -> ReattachReport { reattached: Vec<ReattachedTab
  { session_id, terminal_id, tmux_session }>, left: Vec<LeftTab { session_id, terminal_id, reason }> }`
  in `connection_service/svc_reattach_live_tmux_tabs.rs` (re-exported from `connection_service`).
- `tab_roster` also exports the property-key constants (`HARNESS_PROPERTY`, `CONVERSATION_ID_PROPERTY`,
  `SESSION_PROPERTY`, `OS_USER_PROPERTY`) and `TabEntry::new` / `LaunchNode::new`.
- Web test ids: `sessions-terminal-tab-new-in-tmux-<kind>` (menu item) and
  `sessions-terminal-tab-tmux-marker-<terminalId>` (tab marker).

## Refactoring Needed

### From @red (TDD Red Phase)

- `TerminalDescriptor` has no launch chain yet: `ListTerminalSessions` maps `launch: Vec::new()`
  (`tddy-terminal-rpc/src/service.rs`). Green adds the field and fills it from the roster, or the
  `an_agent_tab_wrapped_in_tmux_…` listing assertion stays red.
- The pre-roster main fallback has no recorded OS user (`.session.yaml` records none); the test runs
  as the daemon's own user. Green decides where that user comes from.
- `tests/support/tab_daemon.rs` duplicates the session-host builder of `session_liveness_acceptance.rs`
  and `claude_cli_tmux_reattach_acceptance.rs`; a shared builder in `tests/support/` would serve all three.

## Decisions & Trade-offs

- **A separate file, not `.session.yaml`** (developer decision): no lock exists on that file, and
  `deny_unknown_fields` would make older daemons reject it.
- **`map<string,string>` properties** per node (developer decision): new wrappers add keys, not
  schema. Typed accessors read the known keys.
- **The OS user is recorded on the `tmux` node**, because tmux servers are per uid and the session
  records no user.
- **Complexity 5:** the boundary is large but one capability (a tab's launch is recorded and
  re-attachable). The developer kept it one node. If red shows it is bigger, split roster-recording
  from tmux tabs before green.
