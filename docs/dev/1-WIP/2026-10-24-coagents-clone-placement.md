# Changeset: A subagent can work on its own clone instead of the main agent's worktree

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto change (attach + entry placement), git worktree seam on the daemon, three crates plus the web
**Branch**: `feature/coagents/clone-placement`, base `feature/coagents/conversation-bubble` (#723) — `#coagents` 5/8
**PR**: _assigned at wave 1_

## Initial Discovery

[2026-10-24-coagents-clone-placement-initial-discovery.md](./2026-10-24-coagents-clone-placement-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ✅ RESOLVED HERE — Daemon-run subagent conversations still write the session worktree — [`2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`](../todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md)

Resolved for the **local** daemon-run case by `clone` placement (the user's choice that `shared`
stays the default is recorded in the PRD). The peer-owned case keeps the roster's "no write-back from
a remote clone" non-goal — wrap narrows the entry to that remainder.

### ⚠ DURING — Automatic subagent commits skip hooks and signing — consent NOT given — [`2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md`](../todo/2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md)

The turn-end commit goes through the same git runner and inherits the same overrides. No new policy.

### ⚠ DURING — An abandoned subagent conversation leaves its branch — [`2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md`](../todo/2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md)

A daemon restart now leaves daemon-run conversation branches too. Not swept here.

### ⚠ DURING — A background job can race the caller sync — [`2026-10-03-a-background-job-can-race-the-caller-sync.md`](../todo/2026-10-03-a-background-job-can-race-the-caller-sync.md)

A coding agent's own background processes are outside every lock. Same exposure, larger surface.

## Responsibility

- `AgentPlacement { SHARED = 0, CLONE = 1 }`; `AttachSessionAgentRequest.placement` (5); `SessionAgentEntry.placement`; `SessionAgentRecord.placement` (serde default `Shared`).
- Daemon open path: for `Clone`, `ConversationWorktrees::ensure` at open; `local_agent_codebase_access` sends the conversation id so `run_in_conversation` applies; a coding agent's cwd / MCP exec target is the conversation worktree.
- `turn_start` sync before every daemon-run turn (reusing `tddy-subagent-worktree`'s sync).
- `commit_turn_changes` — one commit per coding-agent turn on the conversation branch.
- `tddy-tools`: `subagent_diff` / `_pull` / `_end` route daemon-run `Clone` conversations to the daemon's `ConversationWorktree` ops; descriptions updated for `Shared`.
- Web: placement select per agent in `CreateSessionAgentPickerSection` and `AgentPicker`; placement on `RosterAgentRow`; `specialized_agents` entries carry the placement (`StartSessionRequest` seeding).

## Boundaries

- No remote placement — n8. No sweeping of orphaned branches. No change to in-jail conversations.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n2 `acp-subagent` (#721) | `CodingAgentLauncher` (cwd, MCP config), daemon open path, `SessionAgentRecord.kind` | passes the conversation worktree as cwd / exec target; adds `placement` beside `kind` | change the launcher contract or the catalog |
| n1, n3, n5 | inherited, not consumed | — | — |

## Draft PR contract

Proto + record fields, `commit_turn_changes`, the placement branch's signatures in the open path,
the web select — bodies `TODO(clone-placement)` — and the failing tests below.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — needs n2's daemon-run coding-agent behaviour
**Concurrent with:** n3, n5, n8
**Blocks:** none

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6

## Affected Packages

- **tddy-service**: `session_agents.proto` (regenerated TS)
- **tddy-changeset**: `SessionAgentRecord.placement`
- **tddy-session-agents**: open path, turn-start sync, turn-end commit
- **tddy-subagent-worktree**: `commit_turn_changes` (if not already expressible with its commit helper)
- **tddy-tools**: diff/pull/end routing
- **tddy-web**: placement select and roster label

## Related Feature Documentation

- [PRD-2026-10-24-coagents-clone-placement.md](../../ft/coder/1-WIP/PRD-2026-10-24-coagents-clone-placement.md)

## Summary

`clone` placement gives a daemon-run conversation its own conversation worktree, synced before each
turn and handed back through `subagent_diff` / `_pull` / `_end`.

## Scope

- [ ] **Implementation** · [ ] **Testing** · [ ] **Package Documentation** · [ ] **Code Quality**

## Technical Changes

### State A (Current)

- `svc_start_hosted_agent_clone.rs:269-334` `local_agent_codebase_access` → `ExecuteToolRequest { conversation_id: "" }` → `run_exec_tool_locally` on the session worktree.
- `local_exec_tools.rs:195-231` `run_in_conversation` applies only when `conversation_id` is set.
- `tddy-subagent-worktree`: worktree at `<session worktree>/tmp/subagent-worktrees/<conv>`, branch `tddy/subagent/<session>/<conv>`; ops in `tddy-agent-launch/src/conversation_worktree_op.rs:41-60` (`Pull`, `PullRange`, `Remove`, `Reset`, `Diff`, `Sync`), reachable over `exec_tools.ConversationWorktree`.
- `tddy-tools/src/subagent_pull.rs:62-85` returns `null` unless the loop runs in-process.
- `AttachSessionAgentRequest` fields 1-4; no placement anywhere.

### State B (Target)

```
attach{placement: CLONE} → record.placement
open(conv) → ensure conversation worktree → coding agent cwd / exec target = it
turn: sync(caller files) → prompt → commit_turn_changes
subagent_pull/diff/end (daemon-run, CLONE) → ConversationWorktree{PullRange|Diff|Remove}
```

## Testing Plan

**Level:** integration — real git worktrees under a temp repo and a stub coding agent that writes a
file (the `tddy-acp-stub` scenario gets a `write_file` action, owned here); `tddy-tools-cli` MCP tests
for diff/pull/end; Cypress component for the select.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/clone_placement_acceptance.rs` (new):

1. `attaching_with_clone_records_the_placement`
2. `a_clone_coding_agent_turn_leaves_the_session_worktree_untouched`
3. `a_clone_turn_starts_on_the_callers_current_files`
4. `a_clone_coding_agent_turn_is_one_commit_on_the_conversation_branch`
5. `a_clone_daemon_run_model_agent_commits_per_mutating_call`

`packages/tddy-tools-cli/tests/subagent_pull_mcp_acceptance.rs` (extend):

6. `subagent_pull_applies_a_daemon_run_clone_conversations_work`
7. `subagent_diff_reads_a_daemon_run_clone_conversation`
8. `subagent_end_takes_the_rest_and_removes_the_worktree`
9. `a_shared_conversation_still_answers_pulled_null`

`packages/tddy-web/cypress/component/CreateSessionAgentPicker.cy.tsx` (extend):

10. `an agent can be attached with clone placement`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Commit per turn for coding agents.** Their edits do not pass through tddy's tools when they run
  with native tools, so per-call commits are impossible; per-turn keeps `subagent_pull` meaningful.
- **Reuse the existing `ConversationWorktree` RPC** rather than adding end/pull/diff to
  `SessionAgentService` — the ops and their auth already exist.

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-coagents-clone-placement-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-session-agents -p tddy-subagent-worktree -p tddy-daemon-rpc -p tddy-tools-cli -p tddy-changeset`; the Cypress spec) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <each> -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
