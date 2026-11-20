# Changeset: Resume restores the whole tab roster

**Date**: 2026-11-19
**Status**: 🚧 In Progress (implementation done; wrap pending)
**Type**: Feature
**Complexity**: 4 — several crates (agent-launch, session-lifecycle, service proto, coding-agent-specs use, session-web), a proto change, per-tab failure semantics
**Stack**: `#session-liveness` 4/4 (PR #961, stack #962) — branch `feature/session-liveness/resume-roster`, base `feature/session-liveness/tool-tab`

## Affected Packages

- **tddy-agent-launch**: `restore_tab_roster` after main's resume; agent-tab argv with `resume`; a
  minted chat id for cursor tabs
- **tddy-service**: proto `ResumeSessionResponse.restored_tabs` (`RestoredTab`)
- **tddy-session-lifecycle**: agent tabs record `conversation_id` / `resumable`; acceptance tests
- **tddy-session-web**: the strip keeps the active tab across Resume; fresh and dropped notices

## Related Feature Documentation

- PRD: [PRD-2026-11-19-resume-restores-tab-roster.md](../../ft/daemon/1-WIP/PRD-2026-11-19-resume-restores-tab-roster.md)
- Discovery: [2026-11-19-resume-restores-tab-roster-initial-discovery.md](2026-11-19-resume-restores-tab-roster-initial-discovery.md)

## Summary

Resume restores every roster tab under its recorded id. Agent tabs resume their own conversation
where the harness can; the rest reopen fresh and say so.

## Responsibility

- `tddy_agent_launch::restore_tab_roster(...) -> Vec<RestoredTab>`, run after main's resume, plus the
  per-kind restore rules.
- Proto `RestoredTab { terminal_id, outcome: RESTORED | FRESH | DROPPED, reason }` on
  `ResumeSessionResponse`.
- Agent-tab argv rendering with `resume` set from the recorded `conversation_id`; cursor tabs minting
  a real chat id; `resumable: false` on harnesses without per-conversation resume.
- Web: the active tab survives Resume, and the notices.

## Boundaries

- Does not change main's resume path.
- Does not record the roster or start tabs under an id — n2 owns both; this node calls them.
- No tool-session tab restore (tool sessions have no secondary tabs; n3's process tab is main).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` tmux-roster | `tabs.yaml` (`TabRoster`, `LaunchNode` accessors), `TabRosterStore`, starting a terminal under a given id, per-terminal tmux attach and re-attach | reads the roster; restarts each entry under its id; re-attaches or relaunches tmux nodes | change the roster schema, the store, or tmux naming |
| `n1` agent-tab-liveness (#958) | the liveness rule | the tests read `is_active` after Resume | — |
| `n3` tool-tab | nothing consumed — the line puts it below | — | — |

## Draft PR contract

The first push publishes `restore_tab_roster` as `todo!()`, the proto `RestoredTab` and its field,
and the failing tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — it restores what n2 records, through n2's start-under-id and tmux re-attach
**Concurrent with:** none
**Blocks:** none

Real dependency edges: `n1 → n2, n3      n2 → n4`

## Prerequisites

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-resume-resume-cli-harness-session.md`
  (136 lines) and `complexity-opencode-cli-spawn-resume-resume-cli-harness.md` — the restore is one
  call after main's resume, in a new `opencode_cli_spawn/restore_tabs.rs`.
- `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-session-resume-session-at-session-coordinate.md`
  — the response field is filled where the response is built; add no branch.
- `packages/tddy-session-web/docs/code-issues/oversized-file-session-main-pane.md` — the web changes
  go in `useSessionTerminals.ts`, not `SessionMainPane`.

### ℹ ANSWERED — agent-conversation tabs follow-ups — [2026-08-30-agent-conversation-tabs-four-follow-ups.md](../todo/2026-08-30-agent-conversation-tabs-four-follow-ups.md)

Those are ACP conversation tabs, not terminal tabs. Unrelated to the roster; recorded so the overlap
is not re-checked.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW — approved 2026-11-19
- [x] TDD Red — failing unit/integration tests
- [x] Implementation (host-recorded chains; every resume path restores the whole roster)
- [x] Per-harness TODOs for non-resumable tabs (opencode, muse, warp) — on `tab_conversation_is_resumable`
- [x] Single-spec Cypress (2 tests), scoped clippy
- [ ] Package documentation — `/pr-wrap`

### Out of scope (developer decision 2026-11-20: "n4 host-only + new nodes")

- **n5 — jail-side tab spawn + a `sandbox` wrapper node.** Nothing starts a secondary tab inside a running
  jail today: a sandboxed session's tabs start host-side. Sandboxing is per tab, so n5 records a `sandbox`
  node only on a tab started in a jail and restores it through the jail's own spawn path.
- **n6 — target tab launch + a target wrapper node** (ssh / Coder), restoring a target tab through the
  session's `RemoteShell`.
- Until then every recorded chain is host-side, and so is its restore. A chain holding a kind restore does
  not know (`sandbox`, a target kind) is DROPPED with a reason naming it, never a failed resume.
  `restore_plan` dispatches per node kind, so n5 and n6 add a kind, not a branch.
- Every resume path restores the whole roster — host, sandboxed and target sessions — with no skip for
  where main runs. Tests: `remote_target_session_acceptance` (a target session's roster, an unknown-kind
  tab dropped); the sandboxed path calls the same `restore_roster_on_resume`, but has no new test (its
  jail harness is the macOS-only one).

## Technical Changes

### State A (Current)

Resume restores main only (`resume.rs`, `cli_harness_tmux.rs:111-199`). Agent tabs render
`--session-id <fresh uuid>` (claude) or `--resume <random uuid>` (cursor), and record nothing.
`activeTerminalId` is local `useState`.

### State B (Target)

After main's resume, `restore_tab_roster` walks `tabs.yaml` and handles each entry:
- live terminal → leave it;
- `agent` (resumable) → start it under its id with `resume`;
- `tmux` → `agent`: re-attach if the tmux session is alive, otherwise a new tmux session plus resume;
- `shell` → fresh;
- `tmux-attach` → re-attach if the user's session exists, otherwise drop;
- `agent` (not resumable) → fresh under its id.

Each entry yields a `RestoredTab`. The web keeps the active id across the stopped → live swap.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs`:
- [x] `resume_restores_every_roster_tab_under_its_recorded_id`
- [x] `a_claude_tab_resumes_its_own_conversation`
- [x] `a_live_tmux_wrapped_tab_is_reattached_not_relaunched_on_resume`
- [x] `a_tmux_wrapped_tab_whose_tmux_session_is_gone_is_relaunched_and_resumed`
- [x] `a_tmux_attach_tab_whose_session_is_gone_is_dropped_and_reported`
- [x] `an_opencode_tab_reopens_fresh_and_is_reported_fresh`
- [x] `a_cursor_tab_resumes_its_minted_chat`
- [x] `a_tab_live_at_resume_is_not_restarted`

`packages/tddy-session-web/cypress/component/ResumeKeepsActiveTabAcceptance.cy.tsx`:
- [x] `keeps the active tab selected across Resume` (written, type-checks, not run — Cypress)

## Unit Tests

- `restore_tab_roster`'s per-kind decision (pure, over a roster and a liveness snapshot).
- Spec rendering: a claude tab with `resume` renders `--resume <id>`.

## Contract as published (red)

Names the changeset left open, as the draft contract fixed them. Module
`packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs.rs`, re-exported as
`tddy_agent_launch::restore_tabs`:

- `RESUMABLE_PROPERTY = "resumable"` (:26) — the `agent` node key; `"false"` marks a harness that
  cannot resume a given conversation, absent means resumable. Defined here, not in n2's
  `tab_roster`, because n2 owns that module.
- `enum TabRestore { LeaveLive, ResumeAgent { harness, conversation_id }, FreshAgent { harness },
  ReattachTmux { session, os_user }, RelaunchTmux { session, os_user, harness, resume_conversation:
  Option<String> }, FreshShell, ReattachTmuxAttach { session, os_user }, Drop { reason } }` (:30).
- `struct LiveTabs { terminals, tmux_sessions: BTreeSet<(os_user, session)> }` (:62), with
  `none` / `with_terminal` / `with_tmux_session` / `terminal_is_live` / `tmux_session_exists`
  (pure data, implemented).
- `fn restore_plan(entry: &TabEntry, live: &LiveTabs) -> TabRestore` (:110) — `todo!()`.
- `struct RosterRestore<'a> { session_id, session_dir, worktree, os_user, cli_manager,
  coding_agents, roster_store: &TabRosterStore }` (:121).
- `async fn restore_tab_roster(RosterRestore<'_>) -> Vec<RestoredTab>` (:140) — `todo!()`, and **not
  yet called**: green adds the one call after main's resume and fills `restored_tabs` where the
  response is built.
- `fn agent_tab_argv(spec, binary, conversation_id, resume: bool) -> Result<Vec<String>, SpecError>`
  (:159) — `todo!()`; the seam the spec-rendering unit tests pin, shared by the tab start (n2's
  `CliManagerTerminalRoster::start`) and the restore.
- Proto (`packages/tddy-service/proto/session.proto`): `ResumeSessionResponse.restored_tabs = 5`
  (:247), `message RestoredTab { terminal_id, outcome, reason }` (:251), `enum RestoredTabOutcome
  { RESTORED_TAB_OUTCOME_UNSPECIFIED, _RESTORED, _FRESH, _DROPPED }` (:259). A live tab left alone
  is reported `RESTORED`. Every `ResumeSessionResponse` literal fills `restored_tabs: Vec::new()`;
  TS regenerated in `tddy-client-web` and `tddy-rust-typescript-tests`.
- Tests: `restore_tabs_tests.rs` (16 unit), `tests/resume_tab_roster_acceptance.rs` (8, serial) over
  the new fixture `tests/resume_roster_support/mod.rs` (stub claude, cursor with `create-chat`,
  opencode), `cypress/component/ResumeKeepsActiveTabAcceptance.cy.tsx` (1).

## Refactoring Needed

### From @red (TDD Red Phase)

- **Gap — start a terminal under a given id.** n2's changeset lists "Starting a terminal under a
  given id" as its responsibility, but its contract publishes no such entry point:
  `start_terminal` / `start_agent_terminal` / `start_tmux_terminal` still mint `Uuid::now_v7()`
  (`tddy-cli-sessions/src/cli_session_manager/terminals.rs`). Only the tmux paths
  (`attach_terminal_to_tmux` / `reattach_terminal`) take a `terminal_id`. `ResumeAgent`,
  `FreshAgent` and `FreshShell` cannot be carried out until n2 adds it; this node does not.
- `RosterRestore.roster_store`: no `TabRosterStore` is reachable from `LaunchSessions` yet; green
  wires the daemon's one instance (n2 owns where it lives).
- `tests/resume_roster_support/mod.rs` repeats the session-host builder of `support/tab_daemon.rs`
  (itself flagged by n2 as duplicating two others); a shared builder in `tests/support/` would serve
  all of them.

## Decisions & Trade-offs

- **Fresh reopen for non-resumable harnesses** (developer decision), marked and reported, with a TODO
  per harness.
- **A main failure fails the resume, a tab failure does not**: the session must come back even when a
  secondary tab cannot.
