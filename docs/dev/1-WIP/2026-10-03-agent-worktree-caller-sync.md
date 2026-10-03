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

### ✅ RESOLVED HERE — A jail can name another session's conversation worktree over the host bridge — [`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md)

`SyncOp` rides the same bridge arm and would have widened the gap into a **read** path (it copies a
session's uncommitted and untracked files into a conversation worktree the same bridge's `Diff` can
read), so the developer chose to close it in this PR. Closed by:

- **Bound host-side.** Each jail now gets its own `DaemonRpcHandler` carrying a `BoundJailSession {
  session_id, session_dir }` — the session the daemon built the jail for, with the session directory
  the daemon itself resolved under the session owner's sessions base. The host keeps only a
  `Weak<DaemonSessionHost>` (`install_sandbox_rpc_bridge`); `sandbox_rpc_handler(session_id,
  session_dir)` builds the bound handler at all three jail-launch sites
  (`svc_start_sandboxed_cursor_cli_session.rs`, `jail_launch_steps.rs`, `relaunch_jail_steps.rs`).
  No roster arm's signature changed; the `ConversationWorktree` arm passes `&self.bound`.
- **Refused when it differs.** `conversation_worktree_from_jail` answers `PermissionDenied` (and logs a
  warning) for a request whose `session_id` is not the bound one.
- **Resolved like the token route.** The worktree is read from the bound `session_dir` through
  `workspace_session::resolve_worktree_root_in_session_dir`, which the token route's
  `resolve_worktree_root_for_session` now also calls; `session_dir_for`
  (`<tddy_data_dir>/sessions/<id>`) is no longer on this path.
- **Test:** `packages/tddy-daemon-rpc/tests/conversation_worktree_host_bridge_acceptance.rs::a_relayed_call_naming_another_session_is_refused`
  (`PermissionDenied`, nothing pulled into the session worktree).

The runner-side rebinding (`tddy-sandbox-runner/src/conversation_root.rs::bind_to_this_session`) is
unchanged and now a second line of defence. Wrap deletes the TODO file.

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

### ⚠ KNOWN GAP (developer, 2026-10-03) — A background job can race the caller sync — [`2026-10-03-a-background-job-can-race-the-caller-sync.md`](../todo/2026-10-03-a-background-job-can-race-the-caller-sync.md)

A background job does not take the worktree lock, so a write it makes between the sync's snapshot
and its `reset --hard` can be lost or misattributed. Fix: postpone the sync while that conversation
has a running job — needs the task registry across crates.

### ⚠ KNOWN GAP — A merge the subagent makes itself is read as a caller sync — [`2026-10-03-a-subagent-made-merge-is-read-as-a-caller-sync.md`](../todo/2026-10-03-a-subagent-made-merge-is-read-as-a-caller-sync.md)

Lineage treats every first-parent merge as a sync. Narrow: a jailed subagent cannot see the common
dir and so cannot merge. Possible fix: a sync marker (ref, or a trailer verified against the snapshot
parent).

### ⚠ DEFERRED (developer consent, 2026-10-03) — Files this change grew past the budget — [`2026-10-03-caller-sync-grew-over-budget-files.md`](../todo/2026-10-03-caller-sync-grew-over-budget-files.md)

After the refactor `subagent.rs` is below its base (2,050 → 2,046); `server.rs` (+10),
`subagent_runtime.rs` (+3), `tddy-session-tool-client/src/lib.rs` (+1) and
`tddy-session-lifecycle/src/connection_service.rs` (+7, the bound-session field) remain above it —
wiring only. A measurement-history row was added to each existing record.

### ℹ ANSWERED — The `ConversationWorktree` call over HTTP is unverified — [`2026-10-01-conversation-worktree-over-http-is-unverified.md`](../todo/2026-10-01-conversation-worktree-over-http-is-unverified.md)

Still unverified; this change adds one more hand-written Connect-JSON arm (`{"sync":{}}`) to the same
function and must follow the same encoding. Not closed here.

## Affected Packages

- **tddy-subagent-worktree**: [README.md](../../packages/tddy-subagent-worktree/README.md), [conversation-worktree.md](../../packages/tddy-subagent-worktree/docs/conversation-worktree.md) — the sync; subagent work = first-parent without merges
- **tddy-service**: `exec_tools.proto` — `SyncOp sync = 15` and its answers (`moreConflicts`); regenerated TS in `tddy-web` and `tddy-rust-typescript-tests` (the refactor pass changed only the `SyncOp` comment, mirrored by hand into both)
- **tddy-session-lifecycle**: `conversation_worktree_op.rs` — the `Sync` arm, one helper per op, the conflict cap; the host bridge bound to its jail's session (`connection_service.rs`, `daemon_rpc_handler.rs`, `svc_host_builders.rs`, `workspace_session.rs`, the three jail-launch sites)
- **tddy-session-tool-client**: `src/conversation.rs` — `sync_conversation_worktree`, its request builder, its HTTP arm
- **tddy-discovery**: [roster-and-subagent-runtime.md](../../packages/tddy-discovery/docs/roster-and-subagent-runtime.md) — `WorktreeSyncPort`, the sync step and notice in `take_turn`, `PromptOutcome::worktree_sync`, `TurnRequest::{without_sync, syncs_worktree}`
- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md) — `ConversationWorktreeSyncPort`, `syncWorktree` on `subagent_prompt` and `subagent_resume`
- **tddy-daemon-rpc**: tests only — the sync suite, and the bound-session refusal in `conversation_worktree_host_bridge_acceptance.rs`

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

- [x] **Implementation**: sync mechanics, lineage switch, wire, discovery step + notice, tddy-tools port + flag
- [x] **Testing**: all acceptance tests below passing
- [ ] **Package Documentation**: crate doc § sync, discovery runtime doc § turn order, feature doc
- [ ] **Code Quality**: scoped clippy/fmt ✅; duplication code issue closed pending wrap (see Technical Debt)

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

1. Snapshot the conversation worktree's uncommitted state as an **unattached** commit on top of the
   tip (scratch index, subject `Changes made outside a tool call`) — `ours`; the tip itself when clean.
   The branch moves onto it only once the merge is clean.
2. `snapshot = inherit::base_commit(caller)`; `taken = second parent of the newest merge on
   `git rev-list --first-parent --merges base..branch``, else `base`.
3. `snapshot^{tree} == taken^{tree}` → `SyncOutcome::Unchanged`.
4. `git merge-tree --write-tree --name-only -z --merge-base <taken> <ours> <snapshot>`: exit 1 →
   `SyncOutcome::Conflicted { paths }`, nothing moved (a dirty worktree stays dirty); exit 1 with no
   paths, a missing or non-hex tree, or any other exit → `WorktreeError::Git` (`parse_merge_tree`).
5. `commit-tree <tree> -p <ours> -p <snapshot> -m "Merge the caller's changes"`; `reset -q --hard
   <merge>` on the branch (`clean -fd` not needed: the tree is the merge's).
6. Facts: `diff --name-status/--numstat <ours> <merge>` → `WorktreeSync { commit, files, lines, paths
   (first SYNC_NOTICE_PATHS = 20, sorted), more_paths }` → `SyncOutcome::Merged`.

Lineage — `ConversationWorktree::subagent_commits()` = `rev-list --first-parent --no-merges --reverse
base..branch`; `pull_range`, `reset_to` (targets + `droppedCommits`) and `diff` (bounds) use it.
`diff` sets `includes_caller_changes` when `rev-list --first-parent --merges from..to` is non-empty.
`pull_into_caller` = `pull_range(&PullRange::default(), &BTreeSet::new())` reshaped into `PullOutcome`.

Wire — `SyncOp {}` at tag 15; answers `{"sync": {…}}`, `{"sync": null}` (no worktree, unchanged, or a
merge that changed no file), `{"conflicts": [≤ 20 paths], "moreConflicts": n}`. The jail host bridge
serves it only for the session its jail was bound to.

Discovery — `WorktreeSyncPort::sync(&self) -> Result<SyncAnswer, SubagentError>` with
`SyncAnswer::{Merged(WorktreeSync), Nothing, Conflicted { paths, more_paths }}`;
`sync_notice(&WorktreeSync)` and `sync_refusal(paths, more_paths, Option<RewindApplied>)` build the
texts; `subagent/turn_start.rs` runs the turn's start (ledger, rewind + reset, sync, appends).

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

- [x] `subagent_commits` and the three listing switches green (existing suites stay green)
- [x] `sync_with_caller` green: merge, unchanged, conflict, dirty, merge base
- [x] wire + daemon arm green
- [x] discovery step, notice and outcome green
- [x] `tddy-tools` port and `syncWorktree` green; TS regenerated (regenerated with the tests, unchanged here)

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

### Verified after the refactor pass (scoped, 2026-10-03)
All pass, 0 failed, 0 ignored: `tddy-subagent-worktree` lib 35, `caller_sync_acceptance` 17,
`conversation_worktree_acceptance` 24, `diff_acceptance` 9, `range_pull_acceptance` 9,
`reset_acceptance` 5, `run_in_conversation_acceptance` 10, `subagent_lineage_acceptance` 10;
`tddy-discovery` lib 97, `caller_sync_acceptance` 12 (every other discovery suite green);
`tddy-session-tool-client` lib 15; `tddy-tools` 57 binaries (lib 78, main 20,
`subagent_sync_worktree_mcp` 3); `tddy-session-lifecycle --lib` 150; `tddy-daemon-rpc`
`conversation_worktree_sync_acceptance` 5, `_exec_tool_` 7, `_reset_` 2, `_diff_` 2, `_range_pull_` 2,
`_host_bridge_` 3, `in_jail_conversation_acceptance` 1; `tddy-service` (proto comment) all green.
`cargo clippy --all-targets -D warnings` and `cargo fmt --check` clean over those seven crates. The
whole workspace is CI's.

### Verified (scoped, green phase)
`cargo clippy --all-targets -D warnings` clean over `tddy-subagent-worktree`, `tddy-service`,
`tddy-session-tool-client`, `tddy-session-lifecycle`, `tddy-discovery`, `tddy-tools`, `tddy-daemon-rpc`.

## Technical Debt & Production Readiness

### Closed pending wrap — `duplication-range-pull-pull-commit.md`

`pull_into_caller` is now `pull_range(&PullRange::default(), &BTreeSet::new())` reshaped into
`PullOutcome` (`worktree.rs`), so the squashed `base..branch` copy of the name-status / numstat /
binary / name-only / `apply_3way` sequence is gone. **Final measurement: 0 repeated lines** (was
~14); `pull_commit` is the one place the sequence lives. The record
`packages/tddy-subagent-worktree/docs/code-issues/duplication-range-pull-pull-commit.md` is left in
place for `/wrap-context-docs` to record in the change history and delete.

One visible effect: `pull_into_caller`'s `files`/`lines` are now the **sum of what each commit
changed** (as `pull_range` reports), not the squashed `base..tip` diff. They differ only when two
commits touch the same file (a create then an edit counts one created and one updated). No suite
pins the squashed counts; `Op::Pull` has no tool caller.

### Decisions made while implementing

- **A merge that changes nothing in the conversation worktree is recorded, reported as
  `SyncOutcome::Unchanged`.** When the caller's files differ from the last state taken in only by
  what the conversation already had (the caller pulled the subagent's work and changed nothing
  else), the merge tree equals the tip's. The merge commit is still made, so the next sync merges
  against it: without it, a later subagent edit of a line the caller had pulled would conflict
  spuriously against the older base. No files changed, so there is nothing to tell the subagent and
  no repeat ledger to clear. The daemon answers `{"sync": null}`. Pinned by
  `caller_sync_acceptance::a_caller_that_only_pulled_*` and
  `after_a_recorded_merge_a_subagent_edit_of_a_pulled_line_merges_without_conflict`.
- **`git_raw` carries the exit code** (`RawOutput::code`), so `merge-tree`'s exit 1 (conflict) is
  told apart from ≥2 (error), as in `base_sync.rs`.
- **`commit_changes` is split** into the locking entry point and `commit_changes_held`, which the
  sync calls for the "Changes made outside a tool call" commit under the lock it already holds
  (`serialise::exclusive` is not re-entrant).
- **`WorktreeSync.paths` comes from `diff --no-renames --name-only -z`**, so each changed path is one
  entry and unusual file names are not quoted; the counts use `--no-renames` too.
- **The `take_turn` step lives in `subagent/worktree_sync.rs::take_in_callers_files`**; `subagent.rs`
  grows by 11 wiring lines.
- **The `syncWorktree` schema property lives in `sync_worktree_choice.rs::sync_worktree_property`**;
  `server.rs` grows by 10 wiring lines (two parser calls, two schema lines). The tool descriptions
  themselves are unchanged.

### Test touched — `tddy-discovery/tests/caller_sync_acceptance.rs`

The fixture `a_conversation_after_one_turn` now takes its first turn `.without_sync()`. Its port
answers `sync_answer` every time, so with `Conflicted` it refused the fixture's own first turn
(`expect("the first turn runs")`) before the test reached its When. No assertion changed. In
production a first turn's sync is answered `{"sync": null}` because no worktree exists yet.

### Pre-existing failures seen in the scoped runs (not touched here)

- `tddy-session-lifecycle`: `sandbox_behavior_acceptance` (5), `sandboxed_claude_cli_acceptance` (5),
  `sandboxed_cursor_cli_acceptance` (4), `sandboxed_session_lifecycle_acceptance` (2) — all
  "sandbox RPC bridge not installed — runtime must call install_sandbox_rpc_bridge" on macOS.
- `cursor_cli_session_acceptance` (1), `sandboxed_codebase_*` (11), `session_sync_livekit_acceptance`
  (6) fail only until `tddy-sandbox-runner` / `tddy-remote-git-repo` are built into `target/debug`;
  with them built they pass.

### Flake fixed (pre-existing)
- `packages/tddy-subagent-worktree/tests/run_in_conversation_acceptance.rs::await_is_treated_as_mutating_and_commits_what_the_background_job_wrote`
  failed once in ~9 whole-crate runs: the Shell call's own commit raced the job's `sleep 0.2`. The job
  now waits on a gate file outside the worktree that the test creates only after the Shell call
  returned, so the late write can only land before the `Await`.

### Refactor pass after validation (2026-10-03)
Every first-pass finding was fixed or explicitly deferred — see **Refactoring Needed**. Behaviour
changes were limited to the listed correctness fixes: the omitted diff bound, the conflicted-dirty
sync, the capped and quoted conflict list (`moreConflicts` on the wire), the rewind note in the
refusal, the merge-tree error guards, and the host-bridge session binding.

## Decisions & Trade-offs

See the PRD's *Decisions made while planning* — all six approved 2026-10-03.

## Refactoring Needed

All first-pass findings, with what became of each (refactor pass, 2026-10-03).

### From /validate-changes
- ✅ **`diff` with no `to` refused a tip that is a sync merge** — an omitted `to` is now the tip with
  no membership check; an explicitly named merge bound is still refused (`diff.rs`). Tests:
  `subagent_lineage_acceptance::a_diff_with_no_upper_bound_runs_to_a_tip_that_is_a_sync`, daemon side
  `conversation_worktree_sync_acceptance::a_diff_to_the_tip_right_after_a_sync_includes_the_callers_changes`.
- ✅ **A conflicted sync moved the branch when the worktree was dirty** — the dirty state is now an
  unattached commit (scratch index, `inherit::uncommitted_state_as_commit`) used as merge-tree's
  `ours`; the branch moves only on a clean merge (`sync.rs`). Test:
  `caller_sync_acceptance::a_conflicted_sync_leaves_the_subagents_uncommitted_work_uncommitted`.
- ✅ **Sync widened the host-bridge forged-session gap into a read path** — closed here, see
  Prerequisites.
- ⏸ **A background job can race the sync** — deferred as a known gap with consent:
  [TODO](../todo/2026-10-03-a-background-job-can-race-the-caller-sync.md).
- ⏸ **A subagent-made merge is read as a sync** — known gap:
  [TODO](../todo/2026-10-03-a-subagent-made-merge-is-read-as-a-caller-sync.md).
- ✅ **git ≥ 2.40 undocumented** — PRD Impact Analysis.
- ✅ **`{"sync": null}` also means "merge recorded, no file changed"** — documented on
  `run_conversation_worktree_op`, the `SyncOp` proto comment (and both generated TS copies), and
  pinned by `caller_sync_acceptance::a_caller_that_only_pulled_*`.
- ⏳ **Package documentation** — still open (TODO list below).

### From /validate-tests
- ✅ Positive controls: `without_a_merge_a_resumed_third_identical_read_is_refused` (discovery);
  the daemon's unchanged case asserts the worktree exists and HEAD did not move; "caller untouched"
  also asserts the merge's paths; `nothing_merged_announces_nothing` asserts the exact final pair.
- ✅ Loose matchers: `is_err()` → the exact `WorktreeError::Git` refusal text; `matches!` → `merged()`;
  full `FileCounts`; exact paths past the limit; whole Connect bodies in the client.
- ✅ Untested wiring: `subagent_prompt_refuses_a_non_boolean_sync_worktree_by_name` over MCP; a sync
  port that fails (`a_sync_that_cannot_reach_the_daemon_refuses_the_turn_before_any_model_call`);
  `a_conflict_after_a_rewind_says_the_rewind_stands`.
- ✅ `sync_from_answer` error branches: not JSON, neither key, malformed sync, `is_error` without
  `error`, `moreConflicts` present/missing — exact messages.
- ✅ Merge recorded but `Unchanged`: `a_caller_that_only_pulled_merges_no_file`,
  `a_caller_that_only_pulled_still_has_the_merge_recorded`,
  `after_a_recorded_merge_a_subagent_edit_of_a_pulled_line_merges_without_conflict`.
- ✅ Fixtures shared through `tddy-subagent-worktree/tests/support/mod.rs` (`a_conversation_that_committed`
  returning the short hashes, `read_in`, `merged`, `refusal_of`, `subject_of`) for the caller-sync and
  lineage suites. The older range-pull / diff / reset suites were left on their own fixtures.
- ✅ Pre-existing flake `run_in_conversation_acceptance::await_is_treated_as_mutating_and_commits_what_the_background_job_wrote`
  — the job now waits on a gate file outside the worktree created after the Shell call returned.
- ✅ Distinct non-zero counts in the `worktreeSync` outcome test; explicit `true` for `syncWorktree`;
  `without_sync_runs_the_turn_on_the_worktree_as_it_stands` renamed `without_sync_turns_the_sync_off`.

### From /validate-prod-ready
- ✅ Blocker — the `diff.rs` default bound (above).
- ✅ Uncapped conflict list — the daemon answers `{"conflicts": [≤ SYNC_NOTICE_PATHS], "moreConflicts": n}`
  (`conflicts_answer`), `SyncAnswer::Conflicted { paths, more_paths }`, and `sync_refusal` names at
  most `SYNC_NOTICE_PATHS` then "and N more".
- ✅ Caller paths raw in the model prompt — a path with a control character is Debug-quoted in the
  notice and the refusal; ordinary paths keep the pinned text.
- ✅ Refusal after a rewind — the refusal says the rewind (and the worktree reset, when one ran)
  stands, only in that case (`RewindApplied`).
- ✅ Exit 1 with no paths / empty or non-hex tree — `WorktreeError::Git` with git's stderr
  (`parse_merge_tree`, unit-tested).
- ✅ No log line — `log::info!` on a merge (commit, file/line counts, path count) and on a conflict
  (path count) in the daemon arm.
- ⏸ Per-turn snapshot cost — accepted as planned (PRD User Impact).
- ⏸ No HTTP timeout on the per-turn call — **left as is**: `tddy-session-tool-client` has no HTTP
  timeout constant or pattern to reuse (its other `reqwest::Client::new()` has none either).
- ✅ `SYNC_MERGE_SUBJECT` exported without a consumer — kept `pub` and pinned by the caller-sync
  suite (the hardcoded subject replaced).

### From /analyze-clean-code
- ✅ `take_turn` 77 → 25 lines — the turn start (ledger clear, rewind + reset, sync, message appends)
  moved to `subagent/turn_start.rs`.
- ✅ `run_conversation_worktree_op` 71 → a one-line-per-op `match` over `pull`, `pull_range`,
  `remove`, `reset`, `diff_of`, `sync`; `{"sync": null}` built once.
- ✅ `sync_with_caller` 28, `merge_tree` 27, `record_merge` 26, `what_the_merge_changed` 26 lines;
  merge-tree exit codes named (`MERGE_TREE_CLEAN`, `MERGE_TREE_CONFLICTED`); `.remove(0)` replaced by
  `short_hash` in `sync.rs` and `reset.rs`; the sync's diff goes through `diff_output`
  (`--no-ext-diff --no-textconv`).
- ✅ Duplication: `lineage.rs` `newest_merge_in`; reset/sync port answer parsing shared in
  `tddy-tools/src/worktree_answer.rs`; the request builders take an `Op` (`conversation_op_request`),
  no `Pull` placeholder. ⏸ The name-status + numstat + `count_change` sequence is still at 4 sites
  (`commit_changes_held`, `pull_commit`, `range_changes`, `what_the_merge_changed`); sync and diff now
  share `diff_output`, the rest is left — each site diffs a different pair (the index, one commit, a
  range, a merge) in a different directory, with different flags.
- ✅ Stale names: `on_branch` → `subagent_commits`; refusals now say "is not the base or one of the
  subagent's commits on branch …" (diff) and "is not one of the subagent's commits on branch …"
  (reset, range pull).
- ✅ `"syncWorktree"` literal → `SYNC_WORKTREE_ARG` (parser and both schemas; the `NOT_A_BOOLEAN`
  message still spells it, being a `&'static str`).
- ⏸ `SyncAnswer` vs `SyncOutcome` naming — kept: one is the daemon's answer as the turn sees it, the
  other the git operation's result, in different crates.
- ⏸ `subagent_resume_tool` 54 lines — not touched beyond the parser call.
- ⏸ File-length gate — deferred with consent:
  [TODO](../todo/2026-10-03-caller-sync-grew-over-budget-files.md).

## Validation Results

### /validate-changes (2026-10-03, first pass)
Ordinary branch (base master); 32 files, all claimed. Scoped suites all pass. Risks: 0 critical,
4 warnings, 3 info — ⚠ `diff.rs:44` default `to` refused when the tip is a sync merge (HIGH, untested);
⚠ background job writes can race `add -A … reset --hard`; ⚠ the dirty-worktree commit precedes the
conflict check, so a conflicted sync moves the branch (breaks the PRD's "nothing moves"); ⚠ Sync on the
host bridge widens the forged-session gap into a **read** path. ℹ a subagent-made merge is misread as a
sync; ℹ git ≥ 2.40 undocumented; ℹ `{"sync": null}` also means "merge recorded, no file changed".
PRD ACs 10 ✅ / 2 ⚠. Scope "Package Documentation" not done.

### /validate-tests (2026-10-03, first pass)
54 tests (37 integration, 17 unit), 0 critical, 14 warnings: absence assertions without positive
control (repeat-ledger control, `{"sync": null}` ambiguity in the daemon suite, "caller untouched" without
asserting a merge, last-message-only notice check); loose matchers (`is_err()`, `matches!`, one
`FileCounts` field, `paths.len()`, one Connect key); untested wiring (`syncWorktree` parser calls and
the sync-port hookup — deletion stays green); untested `sync_from_answer` error branches and a failing
sync port; the merge-recorded-but-`Unchanged` case; fixtures copied across 4–6 suites. Pre-existing
flake root-caused (the Shell call's own commit races the job's `sleep 0.2`).

### /validate-prod-ready (2026-10-03, first pass)
❌ 1 blocker (the `diff.rs:44` default-bound bug), 7 warnings: uncapped conflict list; caller paths raw in
the model prompt; refusal after a rewind not stated; exit-1-with-no-paths / empty tree unguarded; no log
line for merge/conflict; per-turn snapshot cost, no HTTP timeout, git ≥ 2.40 undocumented;
`SYNC_MERGE_SUBJECT` exported with no consumer. No mocks, TODOs, debug output or new fallbacks.

### /analyze-clean-code (2026-10-03, first pass)
C (7/10). Must refactor (pre-existing overruns grown): `take_turn` 63 → 77, `run_conversation_worktree_op`
63 → 71. Needs attention: `sync_with_caller` 42, `merge_tree` 42, `what_the_merge_changed` 41,
`subagent_resume_tool` 54. Duplication: `duplication-range-pull-pull-commit.md` **resolved** (~14 → 0);
new — `lineage.rs` rev-list-merges ×2, reset/sync port answer parsing, name-status+numstat+count_change
×4. Stale names/refusals (`on_branch`, "not a commit of branch … after its base"); `SyncAnswer` vs
`SyncOutcome` naming; unnamed merge-tree exit codes; `"syncWorktree"` literal ×4.
File-length gate: `subagent.rs` 2050 → 2096, `server.rs` 2800 → 2810, `subagent_runtime.rs` 732 → 735,
`tddy-session-tool-client/src/lib.rs` 1110 → 1111.

### Developer decisions on the findings (2026-10-03)
- Diff with no `to` diffs to the tip even when it is a sync merge, flagged `includesCallerChanges`; an
  explicit merge bound stays refused.
- The host-bridge session binding is fixed **in this PR** (it was ⚠ DURING; now ⛔ in scope).
- Residual file-length growth after moving the turn phase out of `take_turn` is **deferred with consent**,
  recorded in a TODO.
- The background-job race is recorded as a TODO.

## TODO

- [x] Record initial discovery (`2026-10-03-agent-worktree-caller-sync-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer proceeded to /green, 2026-10-03)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [x] Run scoped tests per touched package (full workspace on CI still to do)
- [x] Validate changes (/validate-changes) — first pass
- [x] Refactor issues from change validation (incl. the host-bridge binding)
- [ ] USER REVIEW — development complete
- [x] Validate tests (/validate-tests) — first pass
- [x] Refactor test issues
- [x] Validate production readiness (/validate-prod-ready) — first pass
- [x] Refactor production readiness issues (HTTP timeout left: no pattern to reuse)
- [x] Analyze code quality (/analyze-clean-code) — first pass
- [x] Refactor code quality issues (file-length growth deferred with consent)
- [ ] Final validation (/validate-changes)
- [x] Linting and formatting (scoped `cargo clippy --all-targets -p <pkg> -- -D warnings`, `cargo fmt` — 2026-10-03, every touched crate)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete, decide next steps
