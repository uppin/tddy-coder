# 2026-10-03 — Every turn takes in the caller's current files

**Type:** Feature

PR [#576](https://github.com/uppin/tddy-coder/pull/576). Cross-package entry:
[2026-10-03-agent-worktree-caller-sync.md](../../../../docs/dev/changesets/2026-10-03-agent-worktree-caller-sync.md).

- `subagent/worktree_sync.rs`: `WorktreeSyncPort`, `SyncAnswer::{Merged, Nothing, Conflicted { paths,
  more_paths }}`, `RewindApplied`, `sync_notice`, `sync_refusal` (≤ `SYNC_NOTICE_PATHS` names then
  `and N more`, control characters Debug-quoted, the rewind stated when it stands),
  `take_in_callers_files`.
- `subagent/turn_start.rs`: `prepare_turn` — ledger clear, reset then rewind, sync (a merge clears the
  repeat ledger again) — and `append_turn_messages` with the notice last. `take_turn` 77 → 25 lines.
- `SubagentConfig::{worktree_sync, with_worktree_sync}`, `SpecializedSubagentSession::syncing_worktree_through`,
  `TurnRequest::{without_sync, syncs_worktree}`, `PromptOutcome::worktree_sync`, `worktreeSync` in
  `prompt_outcome_json`.
- Tests: `tests/caller_sync_acceptance.rs` (12), with reset and sync ports sharing one event log.

`oversized-file-subagent`: 2,050 → 2,046; `oversized-file-subagent-runtime`: 732 → 735 — rows recorded.
