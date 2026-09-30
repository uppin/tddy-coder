# Changeset: A caller can read the diff between any two points of a subagent conversation

**Date**: 2026-09-30
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#agent-worktree` 3/4 — branch `feature/agent-worktree/diff`, base `feature/agent-worktree/rewind-reset`
**PR**: [#562](https://github.com/uppin/tddy-coder/pull/562)

## Initial Discovery

[2026-09-30-agent-worktree-diff-initial-discovery.md](./2026-09-30-agent-worktree-diff-initial-discovery.md)

## Responsibility

`subagent_diff { sessionId, from?, to? }` — the unified diff between two commits of a conversation,
with whole-range counts and a 64 KiB text cap. It owns:

- `ConversationWorktree::diff(from: Option<&str>, to: Option<&str>) -> Result<ConversationDiff, WorktreeError>`
  and `ConversationDiff { from, to, files, lines, diff, truncated }`, plus `DIFF_TEXT_CAP_BYTES`;
- the `DiffOp` case of `ConversationWorktreeRequest.op` and its daemon arm;
- `tddy_session_tool_client::diff_conversation_worktree` (beside n1's calls in `src/conversation.rs`);
- the `subagent_diff` MCP tool (`tddy-tools/src/subagent_diff.rs`), its schema, its routing, and its
  allowlist entry;
- the regenerated `packages/tddy-web/src/gen/exec_tools_pb.ts` for `DiffOp`.

## Boundaries

- Read-only: no reset, no pull, nothing written to either worktree.
- No path filter, no diff against the caller's worktree.
- Does not touch `tddy-discovery` — the diff is not part of a turn.
- Does not depend on `rewind-reset`'s behaviour; it sits after it only because the stack is a line.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `isolated-edits` (#560) | `ConversationWorktree::{base, root}`, `FileCounts`, `LineCounts`, the change-facts parser and the git runner | `diff` reuses the parser for its counts | re-implement counting or change `WorktreeChange` |
| `isolated-edits` (#560) | `ConversationWorktreeRequest` op oneof, the handler, the client call and the jail relay | adds `DiffOp diff = 13` and its arm; `subagent_diff` sends it through the same client call | add a second RPC or relay entry |
| `rewind-reset` (#561) | `ResetOp reset = 12` | nothing — named so field number 13 is not reused | touch reset |

**Sequencing fact:** every test drives a worktree with real commits made by `isolated-edits`'
`ensure` / `commit_changes`, so this node goes green only after `isolated-edits` is green. It does not
wait on `rewind-reset`: the "dropped commit is refused" test makes its dropped commit with plain
`git reset --hard` in the fixture, not through `reset_to`.

## Draft PR contract

Published as this PR's second commit: `ConversationWorktree::diff`, `ConversationDiff`,
`DIFF_TEXT_CAP_BYTES` (`src/diff.rs`), `DiffOp diff = 13`, `diff_conversation_worktree`, and the
`subagent_diff` module — definition and handler, **not routed**, so its advertisement test fails until
green routes it — with behaviour bodies `todo!()` under `// TODO(diff)`, plus the failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — needs `isolated-edits`' worktree behaviour; nothing from `rewind-reset`
**Concurrent with:** `feature/agent-worktree/rewind-reset`
**Blocks:** nothing

    n1 → n2, n3, n4      n2 → n4

## Prerequisites

### ⚠ DURING — Seven files over budget — [`2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md`](../todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md)

`subagent_diff` goes in its own module; `tddy-tools/src/server.rs` (code issue
`packages/tddy-tools/docs/code-issues/oversized-file-server.md`) grows by the route line only.

## Affected Packages

- **tddy-subagent-worktree**: `diff`
- **tddy-service**: `exec_tools.proto` — `DiffOp`
- **tddy-daemon-rpc**: the `Diff` arm
- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md) — `subagent_diff`
- **tddy-sandbox-recipes**: allowlists `mcp__tddy-tools__subagent_diff`

## Related Feature Documentation

- [PRD-2026-09-30-agent-worktree-diff.md](../../ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-diff.md)

## Summary

`subagent_diff` returns the diff between any two commits of a conversation — base..tip by default —
with its file and line counts.

## Background

See the PRD.

## Scope

- [ ] **Implementation**: `diff`, `DiffOp`, `subagent_diff`
- [ ] **Testing**: all acceptance tests below passing
- [ ] **Package Documentation**: crate doc § diff, tddy-tools README tool table
- [ ] **Code Quality**: scoped clippy/fmt

## Technical Changes

### State A (Current)

After `isolated-edits` and `rewind-reset`: a conversation branch `base..tip`, commits named by
`worktreeChange.commit`, and a `ConversationWorktree` RPC with `Pull`, `Remove`, `Reset`. There is no
way to read a change's content short of reading files.

### State B (Target)

- `diff(from, to)`: resolve `from` (default base) and `to` (default tip); both must be on
  `base..tip` or be the base; `from` must be an ancestor of `to` (`merge-base --is-ancestor`); run
  `git diff --binary --no-ext-diff --no-textconv from to` for the text (cut at the last line boundary
  under `DIFF_TEXT_CAP_BYTES` = 64 KiB) and `--name-status` / `--numstat` for the counts.
- Wire: `DiffOp { string from = 1; string to = 2; }`; result JSON as in the PRD.
- MCP: `subagent_diff { sessionId, from?, to? }`, allowed while a turn is running (does not take the
  conversation's session lock).

### Delta

- **tddy-subagent-worktree** — `diff`, `ConversationDiff`, `DIFF_TEXT_CAP_BYTES`.
- **tddy-service** — `DiffOp diff = 13`.
- **tddy-daemon-rpc** — the arm.
- **tddy-tools** — `subagent_diff` module + route + schema.
- **tddy-sandbox-recipes** — allowlist.

## Implementation Milestones

- [ ] `diff` green against real repositories
- [ ] daemon arm green
- [ ] MCP tool green; allowlist updated

## Testing Plan

Mechanics against real repositories — the assertions are on git's own output, so a double would
assert nothing. The daemon arm in the exec-tool acceptance style. The MCP tool over the real stdio
wire for advertisement, argument validation and the no-worktree refusal.

### Acceptance Tests

All fail on this branch. The mechanics and daemon suites fail first on #560's unimplemented
`ConversationId::parse` / `ExecuteTool` routing — the sequencing fact above.

#### tddy-subagent-worktree — `packages/tddy-subagent-worktree/tests/diff_acceptance.rs` (9)
- `with_no_bounds_the_diff_runs_from_the_base_to_the_tip`
- `from_and_to_select_the_changes_after_from_through_to`
- `the_diff_text_is_what_git_itself_reports_for_the_range`
- `the_counts_describe_the_whole_range`
- `a_diff_past_the_cap_is_cut_at_a_line_and_marked_truncated_with_whole_range_counts`
- `a_commit_outside_the_conversation_is_refused`
- `a_commit_dropped_from_the_branch_is_refused`
- `a_from_that_is_not_an_ancestor_of_to_is_refused`
- `a_binary_change_shows_as_binary`

#### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/conversation_worktree_diff_acceptance.rs` (2)
- `diff_answers_with_the_conversations_changes`
- `diff_on_a_conversation_without_a_worktree_is_refused` (`FailedPrecondition`)

#### tddy-tools — `packages/tddy-tools/tests/subagent_diff_mcp_acceptance.rs` (3)
- `subagent_diff_is_advertised`
- `a_diff_of_a_conversation_that_never_wrote_is_refused`
- `a_diff_of_an_unknown_conversation_is_refused`

#### tddy-sandbox-recipes — unit test in `packages/tddy-sandbox-recipes/src/claude_cli.rs`
- `subagent_diff_is_allowlisted_wherever_subagent_cancel_is`

### Unit tests
- `tddy-session-tool-client/src/conversation.rs` — `a_diff_names_both_bounds_and_leaves_an_omitted_one_empty`

### Verified
Scoped: `cargo clippy --all-targets -D warnings` clean over `tddy-subagent-worktree`, `tddy-service`,
`tddy-session-tool-client`, `tddy-tools`, `tddy-daemon-rpc`, `tddy-sandbox-recipes`.

## Technical Debt & Production Readiness

(populated during development)

## Decisions & Trade-offs

- **`from` exclusive, `to` inclusive** — git's `A..B`, so a caller can pass the same two hashes to
  `git diff` and get the same answer.
- **Counts over the whole range even when truncated** — the counts are the summary, the text is the
  detail.

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

- [x] Record initial discovery (`2026-09-30-agent-worktree-diff-initial-discovery.md`)
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
