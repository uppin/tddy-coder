# 2026-10-03 — A conversation worktree merges in its caller's current files

**Type:** Feature

PR [#576](https://github.com/uppin/tddy-coder/pull/576). Cross-package entry:
[2026-10-03-agent-worktree-caller-sync.md](../../../../docs/dev/changesets/2026-10-03-agent-worktree-caller-sync.md).

- `ConversationWorktree::sync_with_caller` (`src/sync.rs`) under the worktree's exclusive lock: the
  worktree's uncommitted state as an unattached commit on the tip (`OUTSIDE_A_TOOL_CALL_SUBJECT`, via
  `inherit::uncommitted_state_as_commit`), the caller snapshot (`inherit::base_commit`), the last
  caller state taken in; equal trees → `SyncOutcome::Unchanged` (branch moved onto `ours` without
  touching files); otherwise `git merge-tree --write-tree --name-only -z --merge-base` read by
  `parse_merge_tree` (exit 0 clean, exit 1 `Conflicted { paths }` with nothing moved, anything else
  `WorktreeError::Git`), `commit-tree -p ours -p snapshot` (`SYNC_MERGE_SUBJECT`), `reset --hard`,
  and `WorktreeSync { commit, files, lines, paths ≤ SYNC_NOTICE_PATHS, more_paths }`. A merge that
  changes no file is recorded and answered `Unchanged`. Needs git ≥ 2.40.
- `src/lineage.rs`: `subagent_commits` (`rev-list --first-parent --no-merges --reverse base..branch`),
  `caller_state_taken_in`, `merges_between`, `newest_merge_in`. `pull_range`, `reset_to` and `diff`
  list through it; refusals read "… is not one of the subagent's commits on branch …" (reset, range
  pull) and "… is not the base or one of the subagent's commits on branch …" (diff).
- `diff`: an omitted `to` is the tip with no membership check; `ConversationDiff::includes_caller_changes`
  (`includesCallerChanges`).
- `pull_into_caller` is `pull_range(&PullRange::default(), &BTreeSet::new())` reshaped into
  `PullOutcome`; its counts are the sum over commits.
- `git_raw` carries the exit code (`RawOutput::code`); `commit_changes_held` for a caller already
  holding the lock; the sync's diffs go through `diff_output`.
- Tests: `caller_sync_acceptance` (17), `subagent_lineage_acceptance` (10), shared fixtures in
  `tests/support/mod.rs`. The `run_in_conversation_acceptance` `Await` flake fixed with a gate file.

Code issue closed: `duplication-range-pull-pull-commit` — ~14 repeated lines (2026-10-01) → **0**
on `4f2a3b66`: the sequence lives only in `range_pull.rs::pull_commit`. Record deleted.
