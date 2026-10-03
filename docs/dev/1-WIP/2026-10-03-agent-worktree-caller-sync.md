# Changeset: A resumed subagent works on the caller's current files

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature
**Branch**: `feature/agent-worktree-caller-sync`, base `master` (`0ce696aa`)

## Initial Discovery

[2026-10-03-agent-worktree-caller-sync-initial-discovery.md](./2026-10-03-agent-worktree-caller-sync-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ✅ RESOLVED HERE — duplication `pull_commit` / `pull_into_caller` — `packages/tddy-subagent-worktree/docs/code-issues/duplication-range-pull-pull-commit.md`

`pull_into_caller` has no tool caller left (`subagent_end` uses `pull_range`), and a squashed
`base..tip` diff would hand a synced caller its own changes back. It becomes "apply every subagent
commit in order" through `pull_range` with an empty ledger, which deletes the duplicated sequence.
Closed at wrap with the final measurement recorded first.

### ⚠ DURING — A jail can name another session's conversation worktree over the host bridge — [`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md)

`SyncOp` rides the same bridge arm and inherits the gap: a forged session id could merge that
session's checkout into that session's own conversation branch. It **reads** the checkout and writes
only inside the conversation worktree, so it is no wider than `Pull` (which writes the checkout).
Recorded, not fixed here — the entry's fix (bind the session host-side) covers every op at once.

### ⚠ DURING — Automatic subagent commits skip hooks and signing — consent NOT given — [`2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md`](../todo/2026-10-01-subagent-commits-skip-hooks-and-signing-without-consent.md)

The sync's merge commit and its "changes made outside a tool call" commit go through the same git
runner, so they inherit the same overrides. This change adds no new policy; the open decision now
covers two more commit kinds.

### ⚠ DURING — `subagent_end` can race a prompt — [`2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md`](../todo/2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md)

The sync runs inside the turn (under the conversation's session lock in `run_turn`), so it is in the
same window as a racing `subagent_end`: an end that removes the worktree mid-sync makes the sync fail
with a git error, refusing that turn. No worse than today's tool call racing the end.

### ⚠ DURING — Files grown past the budget — [`2026-10-01-files-the-agent-worktree-change-grew-past-the-budget.md`](../todo/2026-10-01-files-the-agent-worktree-change-grew-past-the-budget.md)

`tddy-discovery/src/subagent.rs`, `tddy-tools/src/server.rs`, `tddy-session-tool-client/src/lib.rs`
are over budget. New logic goes into new modules (`subagent/worktree_sync.rs`,
`tddy-tools/src/worktree_sync_port.rs` + `sync_worktree_choice.rs`, the existing client
`conversation.rs`); the three files grow by wiring lines only.

### ℹ ANSWERED — The `ConversationWorktree` call over HTTP is unverified — [`2026-10-01-conversation-worktree-over-http-is-unverified.md`](../todo/2026-10-01-conversation-worktree-over-http-is-unverified.md)

Still unverified; this change adds one more hand-written Connect-JSON arm (`{"sync":{}}`) to the same
function and must follow the same encoding. Not closed here.

## Affected Packages

- **tddy-subagent-worktree**: [README.md](../../packages/tddy-subagent-worktree/README.md), [conversation-worktree.md](../../packages/tddy-subagent-worktree/docs/conversation-worktree.md) — the sync; subagent work = first-parent without merges
- **tddy-service**: `exec_tools.proto` — `SyncOp sync = 15`; regenerated TS in `tddy-web` and `tddy-rust-typescript-tests`
- **tddy-session-lifecycle**: `conversation_worktree_op.rs` — the `Sync` arm
- **tddy-session-tool-client**: `src/conversation.rs` — `sync_conversation_worktree`, its request builder, its HTTP arm
- **tddy-discovery**: [roster-and-subagent-runtime.md](../../packages/tddy-discovery/docs/roster-and-subagent-runtime.md) — `WorktreeSyncPort`, the sync step and notice in `take_turn`, `PromptOutcome::worktree_sync`, `TurnRequest::{without_sync, syncs_worktree}`
- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md) — `ConversationWorktreeSyncPort`, `syncWorktree` on `subagent_prompt` and `subagent_resume`
- **tddy-daemon-rpc**: tests only

## Related Feature Documentation

- [PRD-2026-10-03-agent-worktree-caller-sync.md](../../ft/coder/1-WIP/PRD-2026-10-03-agent-worktree-caller-sync.md)
- [managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md)

## Summary

Before every subagent turn, the conversation worktree merges in the caller's current files (`HEAD`
+ uncommitted), so a subagent resumed after the caller edited code works on that code. The subagent
is told what changed; a conflict refuses the turn. Every operation that lists the subagent's work
stops seeing the caller's merged changes as subagent commits.

## Background

See the PRD: the hand-off "subagent finishes → caller pulls and edits → caller resumes" leaves the
subagent on a stale tree.

## Scope

- [ ] **Implementation**: sync mechanics, lineage switch, wire, discovery step + notice, tddy-tools port + flag
- [ ] **Testing**: all acceptance tests below passing
- [ ] **Package Documentation**: crate doc § sync, discovery runtime doc § turn order, feature doc
- [ ] **Code Quality**: scoped clippy/fmt; duplication code issue closed

## Technical Changes

### State A (Current)

- The branch is `base → c1 → c2 …`, each `cN` one mutating call (`worktree.rs::commit_changes`).
- Every listing walks `base..branch` unfiltered: `range_pull::commits_after_base`, `reset::reset_to`,
  `diff::diff`; `pull_into_caller` is a tree diff `base..tip`.
- `take_turn` (`tddy-discovery/src/subagent.rs:1907-1969`): reset (before `rewind_to`), `rewind_to`,
  push prompt / correction / replacement, loop. No step reads the caller's current files.
- `subagent_prompt` parses no worktree option; `subagent_resume` parses `resetWorktree`.
- `ConversationWorktreeRequest.op`: pull 10, remove 11, reset 12, diff 13, pull_range 14.

### State B (Target)

```
take_turn
  validate
  reset   (rewind && resets_worktree)            ← unchanged, before rewind_to
  rewind_to
  sync    (syncs_worktree && port present)        ← NEW
     Merged(s)      → forget_earlier_calls; remember s for the notice and the outcome
     Nothing        → —
     Conflicted(ps) → Err(refusal naming ps and the three ways out)
  push prompt / correction / replacement
  push notice(s)  (last)                          ← NEW
  loop
  outcome.worktree_sync = s
```

Mechanics — `ConversationWorktree::sync_with_caller() -> Result<SyncOutcome, WorktreeError>`, under
`serialise::exclusive(root)`:

1. `add -A`; if the conversation worktree is dirty, commit as subagent work, subject
   `Changes made outside a tool call`.
2. `snapshot = inherit::base_commit(caller)`; `taken = second parent of the newest merge on
   `git rev-list --first-parent --merges base..branch``, else `base`.
3. `snapshot^{tree} == taken^{tree}` → `SyncOutcome::Unchanged`.
4. `git merge-tree --write-tree --name-only -z --merge-base <taken> <tip> <snapshot>`: exit 1 →
   `SyncOutcome::Conflicted { paths }`, nothing moved; exit ≥2 → error.
5. `commit-tree <tree> -p <tip> -p <snapshot> -m "Merge the caller's changes"`; `reset -q --hard
   <merge>` on the branch (`clean -fd` not needed: the tree is the merge's).
6. Facts: `diff --name-status/--numstat <tip> <merge>` → `WorktreeSync { commit, files, lines, paths
   (first SYNC_NOTICE_PATHS = 20, sorted), more_paths }` → `SyncOutcome::Merged`.

Lineage — `ConversationWorktree::subagent_commits()` = `rev-list --first-parent --no-merges --reverse
base..branch`; `pull_range`, `reset_to` (targets + `droppedCommits`) and `diff` (bounds) use it.
`diff` sets `includes_caller_changes` when `rev-list --first-parent --merges from..to` is non-empty.
`pull_into_caller` = `pull_range(&PullRange::default(), &BTreeSet::new())` reshaped into `PullOutcome`.

Wire — `SyncOp {}` at tag 15; answers `{"sync": {…}}`, `{"sync": null}` (no worktree or unchanged),
`{"conflicts": [paths]}`.

Discovery — `WorktreeSyncPort::sync(&self) -> Result<SyncAnswer, SubagentError>` with
`SyncAnswer::{Merged(WorktreeSync), Nothing, Conflicted(Vec<String>)}`; `sync_notice(&WorktreeSync)`
builds the message text.

### Delta

- **tddy-subagent-worktree** — `src/sync.rs` (`WorktreeSync`, `SyncOutcome`, `SYNC_NOTICE_PATHS`,
  `sync_with_caller`), `src/lineage.rs` (`subagent_commits`, `synced_caller_state`), the three listing
  switches, `ConversationDiff::includes_caller_changes`, `pull_into_caller` through `pull_range`.
- **tddy-service** — `SyncOp`; regenerated TS (both gen dirs).
- **tddy-session-lifecycle** — the `Sync` arm.
- **tddy-session-tool-client** — `sync_conversation_worktree`, `conversation_sync_request`, the HTTP arm.
- **tddy-discovery** — `subagent/worktree_sync.rs`, `SubagentConfig::{worktree_sync, with_worktree_sync}`,
  `SpecializedSubagentSession::syncing_worktree_through`, `TurnRequest::{without_sync, syncs_worktree}`,
  `PromptOutcome::worktree_sync`, `prompt_outcome_json`'s `worktreeSync`, the `take_turn` step.
- **tddy-tools** — `worktree_sync_port.rs`, `sync_worktree_choice.rs`, wiring in
  `subagent_config_for_conversation`, both schemas.

## Implementation Milestones

- [ ] `subagent_commits` and the three listing switches green (existing suites stay green)
- [ ] `sync_with_caller` green: merge, unchanged, conflict, dirty, merge base
- [ ] wire + daemon arm green
- [ ] discovery step, notice and outcome green
- [ ] `tddy-tools` port and `syncWorktree` green; TS regenerated

## Testing Plan

Mechanics against real repositories with the existing `tests/support` builder — the assertions are
about what git recorded. The daemon arm in the existing `ADaemonWithASession` style. The loop with
`wiremock` plus recording ports that share one event log, so the **order** reset → sync → first model
call is asserted directly. The MCP surface for the advertised `syncWorktree`.

### Acceptance Tests

All fail on this branch for missing implementation (`todo!()`, `Unimplemented`, or the sync never
being asked for). Tests marked **guard** pass by design — they pin the paths that must not change.

#### tddy-subagent-worktree — `packages/tddy-subagent-worktree/tests/caller_sync_acceptance.rs` (13)
- `a_caller_edit_is_merged_into_the_conversation_worktree`
- `the_subagents_unpulled_commits_survive_the_sync`
- `the_merge_has_the_subagents_tip_as_its_first_parent`
- `changes_the_caller_already_pulled_merge_without_conflict`
- `a_caller_that_has_not_changed_merges_nothing`
- `a_second_sync_with_nothing_new_from_the_caller_merges_nothing`
- `a_sync_merges_against_the_last_caller_state_taken_in`
- `a_commit_the_caller_made_since_the_start_is_merged`
- `a_conflict_merges_nothing_and_names_the_paths`
- `the_sync_reports_what_the_merge_changed`
- `paths_past_the_limit_are_counted_not_listed`
- `uncommitted_conversation_changes_are_committed_as_subagent_work_first`
- `a_sync_leaves_the_callers_index_branch_and_files_untouched`

#### tddy-subagent-worktree — `packages/tddy-subagent-worktree/tests/subagent_lineage_acceptance.rs` (9)
Fixture: c1 → sync merge of a caller edit → c2.
- `the_subagents_commits_leave_out_the_merge_and_the_callers_snapshot`
- `a_range_pull_after_a_sync_applies_only_the_subagents_commits`
- `pull_into_caller_never_hands_the_caller_its_own_changes_back`
- `a_reset_never_lists_the_merge_as_dropped`
- `a_reset_past_a_sync_drops_it_and_the_next_sync_takes_the_caller_in_again`
- `a_merge_commit_is_refused_as_a_reset_target`
- `a_merge_commit_is_refused_as_a_diff_bound`
- `a_diff_spanning_a_sync_says_it_includes_caller_changes`
- `a_diff_with_no_sync_in_its_range_does_not` — **guard** in intent; fails today only because the
  fixture's sync is unimplemented

#### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/conversation_worktree_sync_acceptance.rs` (4)
- `sync_merges_the_session_worktrees_edit_into_the_conversation`
- `sync_with_an_unchanged_session_worktree_merges_nothing`
- `sync_on_a_conversation_without_a_worktree_merges_nothing`
- `sync_names_the_conflicting_paths_and_merges_nothing`

#### tddy-discovery — `packages/tddy-discovery/tests/caller_sync_acceptance.rs` (9)
Recording reset and sync ports share one event log; each event notes the model requests seen.
- `a_prompt_takes_in_the_callers_files_before_the_first_model_call`
- `a_resume_takes_them_in_too`
- `a_rewind_resets_the_worktree_before_it_syncs`
- `a_turn_without_sync_asks_for_none` — **guard**
- `a_merge_is_announced_last_before_the_turn_runs`
- `nothing_merged_announces_nothing` — **guard**
- `the_outcome_reports_the_merge`
- `a_conflict_refuses_the_turn_before_any_model_call_naming_the_paths`
- `a_merge_lets_the_subagent_read_a_file_again` — a merge clears the repeated-call ledger (today: 2
  reads reach the codebase, the third is refused as a repeat)

#### tddy-tools — `packages/tddy-tools/tests/subagent_sync_worktree_mcp.rs` (2)
- `subagent_prompt_advertises_sync_worktree_as_a_boolean`
- `subagent_resume_advertises_sync_worktree_as_a_boolean`

### Unit tests
- `tddy-discovery/src/subagent/worktree_sync.rs` — `the_notice_names_the_files_and_the_lines`,
  `the_notice_counts_the_paths_it_does_not_name`, `the_refusal_names_every_conflicted_path_and_the_ways_forward`
- `tddy-discovery/src/subagent/turn_request.rs` — `a_turn_takes_in_the_callers_files_by_default`,
  `without_sync_runs_the_turn_on_the_worktree_as_it_stands`
- `tddy-discovery/src/subagent_runtime.rs` — `a_turn_that_took_in_the_callers_files_reports_the_sync`,
  `a_turn_that_took_nothing_in_has_no_sync_key` (**guard**)
- `tddy-tools/src/worktree_sync_port.rs` — merged / null / conflicts / error answers (4)
- `tddy-tools/src/sync_worktree_choice.rs` — default, `false`, non-boolean (3)
- `tddy-session-tool-client/src/conversation.rs` — `a_sync_names_its_conversation_and_asks_for_a_sync`,
  `a_sync_travels_over_http_as_its_own_key`, `a_reset_still_travels_over_http_with_its_commit`
  (**guard** for the `connect_json_body` extraction)

### Surface published with the tests
`ConversationWorktree::{sync_with_caller, subagent_commits}`, `caller_state_taken_in` (crate-private),
`SyncOutcome`, `WorktreeSync`, `SYNC_NOTICE_PATHS`, `SYNC_MERGE_SUBJECT`, `OUTSIDE_A_TOOL_CALL_SUBJECT`,
`ConversationDiff::includes_caller_changes`; `SyncOp sync = 15` (both TS gens regenerated);
`sync_conversation_worktree`, `conversation_sync_request`, `connect_json_body` (extracted from
`ask_daemon_over_http`, behaviour-preserving); `WorktreeSyncPort`, `SyncAnswer`, `sync_notice`,
`sync_refusal`, `SubagentConfig::with_worktree_sync`, `syncing_worktree_through`,
`TurnRequest::{without_sync, syncs_worktree}`, `PromptOutcome::worktree_sync`;
`ConversationWorktreeSyncPort`, `with_sync_worktree_choice`. Behaviour bodies are `todo!()` under
`TODO(caller-sync)`; the `take_turn` step, the daemon `Sync` arm, the `syncWorktree` schema properties
and the port wiring in `subagent_config_for_conversation` are left for green because publishing them
would make their own tests pass unimplemented.

### Verified (scoped)
`cargo clippy --all-targets -D warnings` clean over `tddy-subagent-worktree`, `tddy-service`,
`tddy-session-tool-client`, `tddy-session-lifecycle`, `tddy-discovery`, `tddy-tools`, `tddy-daemon-rpc`.

## Technical Debt & Production Readiness

(populated during development)

## Decisions & Trade-offs

See the PRD's *Decisions made while planning* — all six approved 2026-10-03.

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

- [x] Record initial discovery (`2026-10-03-agent-worktree-caller-sync-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer proceeded to /green, 2026-10-03)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests per touched package; full workspace on CI
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
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete, decide next steps
