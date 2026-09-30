# Changeset: Going back in a subagent conversation takes its worktree back too

**Date**: 2026-09-30
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#agent-worktree` 2/4 — branch `feature/agent-worktree/rewind-reset`, base `feature/agent-worktree/isolated-edits`

## Initial Discovery

[2026-09-30-agent-worktree-rewind-reset-initial-discovery.md](./2026-09-30-agent-worktree-rewind-reset-initial-discovery.md)

## Responsibility

A rewind (`subagent_resume{fromMessageId}`) resets the conversation worktree to the rewind point's
commit, by default, with `resetWorktree: false` to opt out and `worktreeReset {to, droppedCommits}`
on the outcome. It owns:

- `ConversationWorktree::reset_to(ResetTarget) -> ResetOutcome` in `tddy-subagent-worktree`;
- the `ResetOp` case of `ConversationWorktreeRequest.op` and its daemon handling;
- the reset port the subagent loop calls — `tddy_discovery::subagent::worktree_reset::{WorktreeResetPort,
  ResetTarget, WorktreeReset}` and `SubagentConfig::worktree_reset`;
- `Transcript::commit_kept_by(&MessageId) -> Option<…>` (the reset target a rewind implies);
- `TurnRequest::keeping_worktree()` / `resets_worktree()`;
- `PromptOutcome::worktree_reset` and its `worktreeReset` JSON;
- `subagent_resume`'s `resetWorktree` property and the `tddy-tools` port adapter.

## Boundaries

- Does not create worktrees, commit, or change `worktreeChange` — `feature/agent-worktree/isolated-edits`.
- Does not report dropped **pulled** commits — there is no ledger until `feature/agent-worktree/range-pull`,
  which adds `droppedPulledCommits` to this node's `worktreeReset`.
- No diff, no pull changes.
- Daemon-run conversations (`ResumeAgentConversation`) are not given a reset; no proto field on
  `session_agents.proto`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `isolated-edits` (#560) | `tddy-subagent-worktree`: `ConversationWorktrees::existing`, `ConversationWorktree::{root, base}`, the fixed git runner | `reset_to` is a new method on its `ConversationWorktree` | change `ensure`, `commit_changes`, or the runner |
| `isolated-edits` (#560) | `ConversationWorktreeRequest` with `oneof op { pull; remove; }`, `ExecToolRpcHandler::conversation_worktree` | adds one `ResetOp reset = 12` case and its arm | renumber or reshape `pull` / `remove` |
| `isolated-edits` (#560) | `MessageDescriptor::worktree_change` recorded at the append site, carrying `commit` | `commit_kept_by` reads the commit off kept entries | change how or where the change is recorded |
| `isolated-edits` (#560) | the per-conversation `ConversationWorktree` client call in `tddy-session-tool-client` | the `tddy-tools` reset adapter sends `Reset` through it | add a second client path |

**Sequencing fact:** this node's daemon and mechanics tests drive a worktree made by the parent's
`ensure` and `commit_changes`; they go green only once `isolated-edits` is green. The discovery tests
inject a recording `WorktreeResetPort` and are greenable on the parent's published surface alone.

## Draft PR contract

The first push after this planning commit publishes `reset_to`, `ResetTarget`, `ResetOutcome`,
`ResetOp`, `WorktreeResetPort`, `SubagentConfig::worktree_reset`, `Transcript::commit_kept_by`,
`TurnRequest::{keeping_worktree, resets_worktree}`, `PromptOutcome::worktree_reset` and the
`resetWorktree` schema property — bodies `// TODO(rewind-reset): implement` — plus the failing tests
below. It is the start of this PR's implementation, not its deliverable.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** partly — the discovery and MCP-schema tests need only published surface;
the mechanics and daemon tests need `isolated-edits`' worktree behaviour
**Concurrent with:** `feature/agent-worktree/diff`
**Blocks:** `feature/agent-worktree/range-pull` (its dropped-pulled-commits report extends this
node's reset)

    n1 → n2, n3, n4      n2 → n4

## Prerequisites

### ⚠ DURING — Seven files over budget — [`2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`](../todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md)

`take_turn` lives in `tddy-discovery/src/subagent.rs` (code issue
`packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md`). The port, the target type and
the reset step go in a new `subagent/worktree_reset.rs`; `take_turn` grows by the call only.

### ⚠ DURING — The resume RPC and its turn budget have no wire-level test — [`2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`](../todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md)

Unaffected: this node changes the MCP `subagent_resume`, not `ResumeAgentConversation`.

## Affected Packages

- **tddy-subagent-worktree**: `reset_to`
- **tddy-service**: `exec_tools.proto` — `ResetOp`
- **tddy-daemon-rpc**: the `Reset` arm
- **tddy-discovery**: [roster-and-subagent-runtime.md](../../packages/tddy-discovery/docs/roster-and-subagent-runtime.md) — `worktree_reset` module, `TurnRequest`, `Transcript`, `PromptOutcome`
- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md) — `resetWorktree`, the reset adapter

## Related Feature Documentation

- [PRD-2026-09-30-agent-worktree-rewind-reset.md](../../ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-rewind-reset.md)

## Summary

A rewind resets the conversation worktree to the commit of the last entry the rewind keeps, or to the
base; a caller can opt out, and the outcome reports what was reset.

## Background

See the PRD.

## Scope

- [ ] **Implementation**: `reset_to`, `ResetOp`, the port, the `take_turn` step, the MCP property
- [ ] **Testing**: all acceptance tests below passing
- [ ] **Package Documentation**: discovery runtime doc § rewind, new crate doc § reset
- [ ] **Code Quality**: scoped clippy/fmt

## Technical Changes

### State A (Current)

After `isolated-edits`: each mutating tool result records `worktree_change.commit` on its transcript
entry. `take_turn` (`subagent.rs:1893`) calls `transcript.rewind_to(point)`, which truncates after the
named message and over the tool results that answer it (`transcript.rs:320-336`), and touches no
files. `TurnRequest` (`subagent/turn_request.rs`) has no worktree choice; `subagent_resume_schema`
(`server.rs:2472`) advertises `fromMessageId`, `correction`, `graceMs`, `maxTurns`.

### State B (Target)

```
take_turn(request)
  validate …
  if rewind_point && request.resets_worktree():
      target = transcript.commit_kept_by(point)          # last kept entry's commit, else Base
      reset  = worktree_reset.reset(target)?              # failure ⇒ Err, transcript untouched
  transcript.rewind_to(point)
  …run the turn…
  outcome.worktree_reset = reset (None when no worktree / opted out / no rewind)
```

- `ResetTarget::{Base, Commit(String)}`; `WorktreeReset { to: String, dropped_commits: u32 }`;
  `WorktreeResetPort::reset(&self, ResetTarget) -> Future<Result<Option<WorktreeReset>, SubagentError>>`
  (`None` = the conversation has no worktree). A session built without a port never resets.
- Mechanics: `reset_to` resolves the target (`Base` → the base commit), counts `target..tip`,
  `reset --hard target`, `clean -fd` (ignored files kept), and returns `ResetOutcome{to, dropped}`.
  A commit that is not on `base..tip` is refused.
- Wire: `ResetOp { string commit = 1; }` (empty = base); result `{"to", "droppedCommits"}` or
  `{"noWorktree": true}`.
- MCP: `resetWorktree` boolean (default `true`); `worktreeReset` in `prompt_outcome_json`.

### Delta

- **tddy-subagent-worktree** — `reset_to`, `ResetTarget`, `ResetOutcome`.
- **tddy-service** — `ResetOp reset = 12`.
- **tddy-daemon-rpc** — the arm.
- **tddy-discovery** — `subagent/worktree_reset.rs`; `SubagentConfig::worktree_reset`;
  `Transcript::commit_kept_by`; `TurnRequest::{keeping_worktree, resets_worktree}`;
  `PromptOutcome::worktree_reset`; `prompt_outcome_json`.
- **tddy-tools** — the adapter (built per conversation, beside the Managed closure);
  `resetWorktree` in the schema and parser.

## Implementation Milestones

- [ ] `reset_to` green against real repositories
- [ ] daemon `Reset` arm green
- [ ] `commit_kept_by` and `take_turn` ordering green
- [ ] MCP property and outcome JSON green

## Testing Plan

Mechanics against real repositories; the daemon arm in `tddy-daemon-rpc`'s exec-tool acceptance
style; the loop with wiremock plus a **recording** `WorktreeResetPort` — the strongest assertion is
*which target was asked for, and that it was asked before the first model request*; the MCP schema over
the real stdio wire.

### Acceptance Tests

#### tddy-subagent-worktree — `packages/tddy-subagent-worktree/tests/reset_acceptance.rs`
- `resetting_to_a_commit_drops_the_commits_after_it`
- `resetting_to_the_base_drops_every_subagent_commit`
- `a_reset_removes_files_the_dropped_calls_created`
- `a_reset_reports_how_many_commits_it_dropped`
- `a_commit_outside_the_conversation_is_refused`

#### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/conversation_worktree_reset_acceptance.rs`
- `reset_moves_the_conversation_worktree_back_to_the_commit`
- `reset_on_a_conversation_without_a_worktree_reports_no_worktree`

#### tddy-discovery — `packages/tddy-discovery/tests/rewind_resets_worktree_acceptance.rs`
- `a_rewind_resets_to_the_commit_of_the_last_kept_tool_result`
- `a_rewind_before_any_commit_resets_to_the_base`
- `a_rewind_with_reset_worktree_false_asks_for_no_reset`
- `the_reset_happens_before_the_resumed_turn_asks_the_model_anything`
- `the_outcome_reports_the_reset`
- `a_conversation_without_a_worktree_reports_no_reset`
- `a_failed_reset_refuses_the_resume_and_keeps_the_history`
- `a_resume_without_a_rewind_asks_for_no_reset`

#### tddy-tools — `packages/tddy-tools/tests/subagent_resume_reset_worktree_mcp.rs`
- `subagent_resume_advertises_reset_worktree`

### Unit tests
- `tddy-discovery/src/subagent/transcript.rs` — `commit_kept_by`: the call's commit is kept with its
  call; a replacement contributes none; ids with no kept commit → `Base`
- `tddy-discovery/src/subagent/turn_request.rs` — resets by default; `keeping_worktree` flips it

## Technical Debt & Production Readiness

(populated during development)

## Decisions & Trade-offs

- **Reset before rewind** so a failed reset leaves the conversation exactly as it was.
- **`clean -fd` keeps ignored files** — build output under the conversation worktree survives a reset,
  which is what a `SHELL cargo build` caller expects.
- **A port, not a dispatch-closure tool name** — reset is not a tool the model can call.

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

## Successor PRs

- `feature/agent-worktree/range-pull` — adds `droppedPulledCommits` to `worktreeReset`

## TODO

- [x] Record initial discovery (`2026-09-30-agent-worktree-rewind-reset-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests per touched package; full workspace on CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete, decide next steps
