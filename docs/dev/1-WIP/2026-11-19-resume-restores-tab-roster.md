# Changeset: Resume restores the whole tab roster

**Date**: 2026-11-19
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — several crates (agent-launch, session-lifecycle, service proto, coding-agent-specs use, session-web), a proto change, per-tab failure semantics
**Stack**: `#session-liveness` 4/4 — branch `feature/session-liveness/resume-roster`, base `feature/session-liveness/tool-tab`

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
- [ ] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [ ] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Per-harness TODOs for non-resumable tabs (opencode, muse, warp)
- [ ] Package documentation, scoped clippy, single-spec Cypress

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
- [ ] `resume_restores_every_roster_tab_under_its_recorded_id`
- [ ] `a_claude_tab_resumes_its_own_conversation`
- [ ] `a_live_tmux_wrapped_tab_is_reattached_not_relaunched_on_resume`
- [ ] `a_tmux_wrapped_tab_whose_tmux_session_is_gone_is_relaunched_and_resumed`
- [ ] `a_tmux_attach_tab_whose_session_is_gone_is_dropped_and_reported`
- [ ] `an_opencode_tab_reopens_fresh_and_is_reported_fresh`
- [ ] `a_cursor_tab_resumes_its_minted_chat`
- [ ] `a_tab_live_at_resume_is_not_restarted`

`packages/tddy-session-web/cypress/component/ResumeKeepsActiveTabAcceptance.cy.tsx`:
- [ ] `keeps the active tab selected across Resume`

## Unit Tests

- `restore_tab_roster`'s per-kind decision (pure, over a roster and a liveness snapshot).
- Spec rendering: a claude tab with `resume` renders `--resume <id>`.

## Decisions & Trade-offs

- **Fresh reopen for non-resumable harnesses** (developer decision), marked and reported, with a TODO
  per harness.
- **A main failure fails the resume, a tab failure does not**: the session must come back even when a
  secondary tab cannot.
