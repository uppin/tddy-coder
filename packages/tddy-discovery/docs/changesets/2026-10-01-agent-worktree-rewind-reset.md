# 2026-10-01 — A rewind resets the conversation worktree before the transcript is cut

**Type:** Feature

`#agent-worktree` 2/4, PR [#561](https://github.com/uppin/tddy-coder/pull/561). Cross-package entry:
[2026-10-01-agent-worktree-rewind-reset.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-rewind-reset.md).

New `subagent/worktree_reset.rs`: `WorktreeResetPort`, with `ResetTarget` and `WorktreeReset`
re-exported from `tddy-subagent-worktree`. `SubagentConfig::{worktree_reset, with_worktree_reset}` and
`SpecializedSubagentSession::resetting_worktree_through` give a session a port; `Transcript::
commit_kept_by` names the target (the last kept entry's commit, else the base) sharing `last_kept_by`
with `rewind_to`; `TurnRequest::{keeping_worktree, resets_worktree}` carry the caller's choice;
`take_turn` resets **before** the cut, so a failed reset returns `Err` with the transcript untouched;
`PromptOutcome::worktree_reset` is serialized as `worktreeReset` by `prompt_outcome_json`.
`TurnStep::FinalAnswer` now boxes its outcome. Runtime doc:
[roster-and-subagent-runtime.md](../roster-and-subagent-runtime.md) § A rewind takes the worktree back.
`subagent.rs` is over the file budget; its split is deferred until after the stack
(`docs/code-issues/oversized-file-subagent.md`).
