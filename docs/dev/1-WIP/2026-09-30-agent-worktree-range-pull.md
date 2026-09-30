# Changeset: A caller pulls a chosen range of a subagent's commits, mid-conversation or at its end

**Date**: 2026-09-30
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#agent-worktree` 4/4 — branch `feature/agent-worktree/range-pull`, base `feature/agent-worktree/diff`

## Initial Discovery

[2026-09-30-agent-worktree-range-pull-initial-discovery.md](./2026-09-30-agent-worktree-range-pull-initial-discovery.md)

## Responsibility

Range pulls with a ledger: `subagent_pull { sessionId, from?, to? }`, the same range on
`subagent_end`, commit-by-commit 3-way application, skipping what was already pulled, and naming
pulled commits a rewind dropped. It owns:

- `ConversationWorktree::pull_range(&PullRange, already_pulled: &BTreeSet<String>) -> Result<RangePullOutcome, WorktreeError>`,
  `PullRange { from, to }`, `RangePullOutcome { commits, skipped, files, lines, conflicts }`;
- the `PullRangeOp pull_range = 14` case of `ConversationWorktreeRequest.op` and its daemon arm;
- `tddy-tools`' `PullLedger` (per conversation, in its own module) and the `subagent_pull` tool;
- `subagent_end`'s `from` / `to` properties and its switch from `Pull` to `PullRange`;
- `droppedPulledCommits` on the MCP `worktreeReset` object, computed in `tddy-tools` from the ledger.

## Boundaries

- Never un-applies anything from the caller's worktree.
- The ledger is not persisted; it lives and dies with the conversation.
- Does not change `Pull` (it stays, unused by `subagent_end`, for any caller that wants the squashed
  base..tip diff), `Reset`, `Diff`, or any discovery type.
- Does not add a field to `rewind-reset`'s `WorktreeReset` / `PromptOutcome` — `droppedPulledCommits`
  is added to the MCP JSON by `tddy-tools`, which owns the ledger.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `isolated-edits` (#560) | `ConversationWorktree`, `pull_into_caller`'s 3-way apply, `PullOutcome`, the op oneof, the client call, the jail relay | `pull_range` applies each commit with the same 3-way step | change `pull_into_caller` or `PullOp` |
| `isolated-edits` (#560) | `subagent_end` and its refusal while a turn runs | adds `from`/`to` and pulls through `PullRange` + the ledger | change `subagent_end`'s removal, refusal or reply shape beyond adding `commits`/`skipped` |
| `rewind-reset` (#561) | `worktreeReset { to, droppedCommits }` in the outcome JSON, `droppedCommits` a **list of short hashes** | intersects it with the ledger to produce `droppedPulledCommits` | reset anything, or change the reset port |
| `diff` (#562) | `DiffOp diff = 13` | nothing — named so field number 14 is free | touch diff |

**Sequencing fact:** the dropped-pulled-commits test needs `rewind-reset`'s reset to actually run, so
it goes green only after #561 is green; the range and ledger tests need only #560's behaviour.

**Planned refinement in #561:** its PRD sketches `droppedCommits` as a count. This node needs the
hashes, so #561's contract commit publishes `droppedCommits` as a list of short hashes (a count is its
length). Recorded here so #561's implementer does not ship the count.

## Draft PR contract

The first push after this planning commit publishes `pull_range`, `PullRange`, `RangePullOutcome`,
`PullRangeOp`, the handler arm, `PullLedger`, the `subagent_pull` tool registration and schema, and
`subagent_end`'s `from` / `to` properties — bodies `// TODO(range-pull): implement` — plus the failing
tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — the dropped-pulled test needs #561's reset; the rest needs #560
**Concurrent with:** none
**Blocks:** nothing

    n1 → n2, n3, n4      n2 → n4

## Prerequisites

### ⚠ DURING — Seven files over budget — [`2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`](../todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md)

`subagent_pull` and `PullLedger` go in their own modules; `tddy-tools/src/server.rs` (code issue
`packages/tddy-tools/docs/code-issues/oversized-file-server.md`) grows by the route line only.

## Affected Packages

- **tddy-subagent-worktree**: `pull_range`
- **tddy-service**: `exec_tools.proto` — `PullRangeOp`
- **tddy-daemon-rpc**: the `PullRange` arm
- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md) — `subagent_pull`, the ledger, `subagent_end` range
- **tddy-sandbox-recipes**: allowlists `mcp__tddy-tools__subagent_pull`

## Related Feature Documentation

- [PRD-2026-09-30-agent-worktree-range-pull.md](../../ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-range-pull.md)

## Summary

A caller pulls any inclusive range of a conversation's commits into its worktree, mid-conversation or
at `subagent_end`; a ledger skips commits already pulled, and a rewind names pulled commits it dropped.

## Background

See the PRD.

## Scope

- [ ] **Implementation**: `pull_range`, `PullRangeOp`, the ledger, `subagent_pull`, `subagent_end` range
- [ ] **Testing**: all acceptance tests below passing
- [ ] **Package Documentation**: crate doc § pull, tddy-tools README tool table
- [ ] **Code Quality**: scoped clippy/fmt

## Technical Changes

### State A (Current)

After #560–#562: `subagent_end` pulls base..tip as one squashed 3-way apply, then removes. There is no
mid-conversation pull and no memory of what was pulled. `worktreeReset.droppedCommits` names what a
rewind dropped.

### State B (Target)

- `pull_range(range, already_pulled)`: list `base..tip` oldest first; resolve `from` (default: the first
  commit not in `already_pulled`) and `to` (default: tip), both inclusive, both on the branch, `from`
  not after `to`; for each commit in the range not in `already_pulled`, `git diff --binary c^ c | git
  apply --3way` in the caller's worktree; collect conflicts; report applied and skipped in order. An
  empty remainder is an empty `commits` list, not an error.
- Wire: `PullRangeOp { string from = 1; string to = 2; repeated string already_pulled = 3; }`.
- `tddy-tools`:
  - `PullLedger { pulled: BTreeSet<String> }` per conversation; `record(&commits)`,
    `dropped(&droppedCommits) -> Vec<String>` (and forgets them).
  - `subagent_pull { sessionId, from?, to? }` — refused while a turn is pending; sends `PullRange` with
    the ledger; records `commits`.
  - `subagent_end { sessionId, from?, to? }` — `PullRange` with the ledger, then `Remove`.
  - The turn outcome's `worktreeReset` gains `droppedPulledCommits` when non-empty.

### Delta

- **tddy-subagent-worktree** — `pull_range`, `PullRange`, `RangePullOutcome`.
- **tddy-service** — `PullRangeOp pull_range = 14`.
- **tddy-daemon-rpc** — the arm.
- **tddy-tools** — `pull_ledger` module, `subagent_pull` module + route + schema, `subagent_end` range,
  outcome annotation.
- **tddy-sandbox-recipes** — allowlist.

## Implementation Milestones

- [ ] `pull_range` green against real repositories
- [ ] daemon arm green
- [ ] ledger unit tests green
- [ ] `subagent_pull`, `subagent_end` range and the dropped-pulled annotation green

## Testing Plan

Mechanics against real repositories; the ledger as pure unit tests; the daemon arm in the exec-tool
acceptance style; the MCP tools over the real stdio wire for advertisement, argument validation, the
running-turn refusal and the no-worktree case.

### Acceptance Tests

#### tddy-subagent-worktree — `packages/tddy-subagent-worktree/tests/range_pull_acceptance.rs`
- `with_no_bounds_every_unpulled_commit_is_applied_in_order`
- `from_and_to_are_both_inclusive`
- `a_commit_already_pulled_is_skipped_and_reported`
- `a_range_with_nothing_left_to_pull_applies_nothing`
- `each_commit_is_applied_three_way_and_conflicts_are_named`
- `a_commit_not_on_the_branch_is_refused`
- `a_from_after_to_is_refused`
- `pulling_leaves_the_conversation_branch_and_worktree_untouched`

#### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/conversation_worktree_range_pull_acceptance.rs`
- `pull_range_hands_exactly_the_range_to_the_session_worktree`
- `pull_range_skips_what_the_caller_already_pulled`

#### tddy-tools — `packages/tddy-tools/tests/subagent_pull_mcp_acceptance.rs`
- `subagent_pull_is_advertised`
- `subagent_end_advertises_from_and_to`
- `pulling_while_a_turn_runs_is_refused`
- `pulling_a_conversation_that_never_wrote_pulls_nothing`

#### tddy-sandbox-recipes — unit test in `packages/tddy-sandbox-recipes/src/claude_cli.rs`
- `subagent_pull_is_allowlisted_wherever_subagent_cancel_is`

### Unit tests
- `tddy-tools/src/pull_ledger.rs` — records, skips on the next selection, `dropped` intersects and
  forgets, empty ledger
- `tddy-tools/src/pull_ledger.rs` — `droppedPulledCommits` is added to `worktreeReset` only when
  non-empty

## Technical Debt & Production Readiness

(populated during development)

## Decisions & Trade-offs

- **Both bounds inclusive** (a pull names the commits it applies), unlike `subagent_diff`'s git-style
  `from..to`; each tool's schema says which.
- **Ledger in `tddy-tools`, selection on the daemon** — the conversation lives in `tddy-tools`, the
  branch on the daemon; the request carries the ledger, so the daemon holds no per-conversation state.
- **Commit-by-commit apply** — a conflict is attributable to one commit, and a skipped commit is never
  folded into a neighbour's diff.

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

- [x] Record initial discovery (`2026-09-30-agent-worktree-range-pull-initial-discovery.md`)
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
