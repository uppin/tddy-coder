# Changeset: Going back in a subagent conversation takes its worktree back too

**Date**: 2026-09-30
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#agent-worktree` 2/4 — branch `feature/agent-worktree/rewind-reset`, base `feature/agent-worktree/isolated-edits`
**PR**: [#561](https://github.com/uppin/tddy-coder/pull/561)

## Initial Discovery

[2026-09-30-agent-worktree-rewind-reset-initial-discovery.md](./2026-09-30-agent-worktree-rewind-reset-initial-discovery.md)

## Responsibility

A rewind (`subagent_resume{fromMessageId}`) resets the conversation worktree to the rewind point's
commit, by default, with `resetWorktree: false` to opt out and `worktreeReset {to, droppedCommits}`
on the outcome. It owns:

- `tddy_subagent_worktree::{ResetTarget, WorktreeReset}` and `ConversationWorktree::reset_to`
  (`src/reset.rs`); `WorktreeReset.dropped_commits` is a **list of short hashes** — `range-pull` needs
  to know *which* commits a rewind dropped;
- the `ResetOp reset = 12` case of `ConversationWorktreeRequest.op` and its daemon handling;
- `tddy_session_tool_client::reset_conversation_worktree` (a function beside n1's calls in
  `src/conversation.rs`, not a variant of n1's `Copy` enum `ConversationWorktreeOp`);
- `tddy_discovery::subagent::{WorktreeResetPort, ResetTarget, WorktreeReset}` (`subagent/worktree_reset.rs`),
  `SubagentConfig::{worktree_reset, with_worktree_reset}`,
  `SpecializedSubagentSession::resetting_worktree_through`, `Transcript::commit_kept_by`,
  `TurnRequest::{keeping_worktree, resets_worktree}`, `PromptOutcome::worktree_reset`;
- the reset step in `take_turn` and `worktreeReset` in `prompt_outcome_json`;
- `tddy-tools`' `ConversationWorktreeResetPort` (`src/worktree_reset_port.rs`) and `subagent_resume`'s
  `resetWorktree` property with its parsing.

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

Published as this PR's second commit — the first push of its implementation, not its deliverable:
every symbol in `## Responsibility` exists with its signature, behaviour bodies `todo!()` under
`// TODO(rewind-reset)`, plus the failing tests below. Two things are deliberately **not** in the
surface because publishing them would make their own tests pass unimplemented: the `resetWorktree`
schema property (advertised and parsed together in green) and the call in `take_turn` (marked
`TODO(rewind-reset)` at the rewind). `TurnStep::FinalAnswer` now boxes its outcome — `PromptOutcome`
grew past clippy's variant-size limit.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — every acceptance suite here drives behaviour `isolated-edits` (#560)
implements: the mechanics and daemon suites need its `ensure` / `commit_changes`, and the discovery
suite needs its append site to record each result's `worktreeChange` (the reset target is read off
those entries). The `TurnRequest` and client-builder unit tests need only this node.
**Concurrent with:** `feature/agent-worktree/diff` (#562), once #560 is green
**Blocks:** `feature/agent-worktree/range-pull` (#563) — it reads `worktreeReset.droppedCommits`

    n1 → n2, n3, n4      n2 → n4

No new crate edges: `tddy-discovery → tddy-subagent-worktree` and the rest are #560's.

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

- [x] **Implementation**: `reset_to`, `ResetOp`, the port, the `take_turn` step, the MCP property
- [x] **Testing**: all acceptance tests below passing
- [ ] **Package Documentation**: discovery runtime doc § rewind, new crate doc § reset
- [x] **Code Quality**: scoped clippy/fmt

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

- `ResetTarget::{Base, Commit(String)}`; `WorktreeReset { to: String, dropped_commits: Vec<String> }` (short hashes, oldest first);
  `WorktreeResetPort::reset(&self, ResetTarget) -> Future<Result<Option<WorktreeReset>, SubagentError>>`
  (`None` = the conversation has no worktree). A session built without a port never resets.
- Mechanics: `reset_to` takes the worktree lock, lists `base..branch`, resolves the target (`Base` →
  the base commit), `reset --hard target`, `clean -fd` (ignored files kept), and returns
  `WorktreeReset{to, dropped_commits}`. A commit that is not on `base..branch` is refused before
  anything moves.
- Wire: `ResetOp { string commit = 1; }` (empty = base); result `{"reset": {"to", "droppedCommits"}}`, or
  `{"reset": null}` when the conversation has no worktree.
- MCP: `resetWorktree` boolean (default `true`); `worktreeReset` in `prompt_outcome_json`.

### Delta

- **tddy-subagent-worktree** — `reset_to`, `ResetTarget`, `WorktreeReset`.
- **tddy-service** — `ResetOp reset = 12`.
- **tddy-session-lifecycle** — the `Reset` arm in `run_conversation_worktree_op`, shared by the exec-tool RPC (`tddy-daemon-rpc`) and the jail bridge.
- **tddy-discovery** — `subagent/worktree_reset.rs`; `SubagentConfig::worktree_reset`;
  `Transcript::commit_kept_by`; `TurnRequest::{keeping_worktree, resets_worktree}`;
  `PromptOutcome::worktree_reset`; `prompt_outcome_json`.
- **tddy-tools** — the adapter (built per conversation, beside the Managed closure);
  `resetWorktree` in the schema and parser.

## Implementation Milestones

- [x] `reset_to` green against real repositories
- [x] daemon `Reset` arm green
- [x] `commit_kept_by` and `take_turn` ordering green
- [x] MCP property and outcome JSON green

## Testing Plan

Mechanics against real repositories; the daemon arm in `tddy-daemon-rpc`'s exec-tool acceptance
style; the loop with wiremock plus a **recording** `WorktreeResetPort` — the strongest assertion is
*which target was asked for, and that it was asked before the first model request*; the MCP schema over
the real stdio wire.

### Acceptance Tests

All fail on this branch; the three marked **guard** pass by design, pinning the paths that must not
reset.

#### tddy-subagent-worktree — `packages/tddy-subagent-worktree/tests/reset_acceptance.rs` (5)
- `resetting_to_a_commit_drops_the_commits_after_it`
- `resetting_to_the_base_drops_every_subagent_commit`
- `a_reset_removes_untracked_files_but_keeps_ignored_ones`
- `resetting_to_the_tip_drops_nothing`
- `a_commit_outside_the_conversation_is_refused_and_nothing_moves`

#### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/conversation_worktree_reset_acceptance.rs` (2)
- `reset_moves_the_conversation_worktree_back_to_the_commit`
- `reset_on_a_conversation_without_a_worktree_reports_no_worktree`

#### tddy-discovery — `packages/tddy-discovery/tests/rewind_resets_worktree_acceptance.rs` (9)
A recording `WorktreeResetPort` notes each target and how many model requests the `wiremock`
provider had seen at that moment.
- `a_rewind_resets_to_the_commit_of_the_last_kept_tool_result`
- `a_rewind_to_the_call_that_made_a_commit_keeps_that_commit`
- `a_rewind_before_any_commit_resets_to_the_base`
- `a_rewind_that_keeps_the_worktree_asks_for_no_reset` — **guard**
- `the_reset_happens_before_the_resumed_turn_asks_the_model_anything`
- `the_outcome_reports_the_reset`
- `a_conversation_without_a_worktree_reports_no_reset` — **guard**
- `a_failed_reset_refuses_the_resume_and_keeps_the_history`
- `a_resume_without_a_rewind_asks_for_no_reset` — **guard**

#### tddy-tools — `packages/tddy-tools/tests/subagent_resume_reset_worktree_mcp.rs` (1)
- `subagent_resume_advertises_reset_worktree_as_a_boolean`

### Unit tests
- `tddy-discovery/src/subagent/turn_request.rs` — `a_rewind_takes_the_worktree_back_by_default`,
  `keeping_the_worktree_opts_a_rewind_out_of_the_reset`
- `tddy-session-tool-client/src/conversation.rs` — `a_reset_to_a_commit_names_the_commit`,
  `a_reset_to_the_base_sends_an_empty_commit`

### Verified
Scoped: `cargo clippy --all-targets -D warnings` clean over `tddy-subagent-worktree`, `tddy-service`,
`tddy-discovery`, `tddy-session-tool-client`, `tddy-tools`, `tddy-daemon-rpc`, `tddy-session-agents`,
`tddy-session-lifecycle`; `cargo test -p tddy-discovery` — every suite green except this node's and
#560's planned red tests.

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

Rebased onto `isolated-edits` @ `b2941c96`; `origin/<base>..HEAD` is this PR's commits only. No stubs
left in `packages/*/src`, no deletions, no `## Dependencies` symbol re-implemented, no `## Boundaries`
breach. Findings: the `take_turn` reset-before-rewind ordering holds (a failed reset leaves history
whole); `commit_kept_by` and `rewind_to` share `last_kept_by`. Fixed in this pass: the
`within_turns` doc comment had been captured by `keeping_worktree`
(`turn_request.rs`); `packages/tddy-rust-typescript-tests/gen/exec_tools_pb.ts` lacked `ResetOp`
after the base regenerated it (`generated-code.sh check` now clean); changeset prose said
`dropped_commits: u32` / `{"noWorktree"}` where the code, tests and proto say a hash list and
`{"reset": null}`.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node gate for 2/4–4/4 on 2026-09-30)
- [x] TDD Red — write failing unit/integration tests
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
