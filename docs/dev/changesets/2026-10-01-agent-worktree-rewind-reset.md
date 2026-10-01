# 2026-10-01 — Going back in a subagent conversation takes its worktree back too

**Type:** Feature

`#agent-worktree` 2/4, base `feature/agent-worktree/isolated-edits` (#560), PR
[#561](https://github.com/uppin/tddy-coder/pull/561). Successors that build on it: `diff`
([#562](https://github.com/uppin/tddy-coder/pull/562)) and `range-pull`
([#563](https://github.com/uppin/tddy-coder/pull/563)). Feature doc:
[managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md) § A rewind takes the
worktree back.

## What was delivered

`subagent_resume { fromMessageId }` used to rewind the transcript and leave the files. It now also
resets the conversation worktree to the commit of the last entry the rewind keeps, or to the
conversation base when none kept a commit. `resetWorktree: false` opts out. The turn outcome reports
`worktreeReset { to, droppedCommits }` (short hashes, oldest first), absent when nothing was reset.
The reset runs before the transcript is cut and before the resumed turn's first model request; a failed
reset refuses the resume with the history intact.

- **`tddy-subagent-worktree`** — `ResetTarget`, `WorktreeReset`, `ConversationWorktree::reset_to`
  (lock, `base..branch` list, refusal of an off-branch commit before anything moves, `reset --hard`,
  `clean -fd` keeping ignored files).
- **`tddy-service`** — `ResetOp reset = 12` on `ConversationWorktreeRequest`.
- **`tddy-session-lifecycle`** — the `Reset` arm of `run_conversation_worktree_op`, shared by the
  exec-tool RPC and the jail bridge.
- **`tddy-session-tool-client`** — `reset_conversation_worktree`.
- **`tddy-discovery`** — `WorktreeResetPort`, `Transcript::commit_kept_by`,
  `TurnRequest::{keeping_worktree, resets_worktree}`, `PromptOutcome::worktree_reset`, the reset step
  in `take_turn`, `worktreeReset` in `prompt_outcome_json`.
- **`tddy-tools`** — `ConversationWorktreeResetPort`, the `resetWorktree` property and its parsing.

## Decisions & Trade-offs

- **Reset before the cut**, so a failed reset leaves the conversation exactly as it was.
- **`clean -fd` keeps ignored files**: build output under the worktree survives a reset.
- **A port, not a tool name** — a reset is not something the model can call.
- **`droppedCommits` is a list of hashes, not a count**, because `range-pull` (#563) must know which
  dropped commits it had already handed to the caller; it adds `droppedPulledCommits` to this object.
- **Managed (daemon-run worktree) conversations only.** `ResumeAgentConversation` is not given a
  reset and `session_agents.proto` is unchanged.

## Deferred

- `subagent.rs` (`tddy-discovery`) and `src/lib.rs` (`tddy-session-tool-client`) grew past the file
  budget again; their splits wait until after the stack. Recorded in
  `packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md` and
  `packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md`; the `docs/dev/todo/`
  entry `2026-10-01-files-the-agent-worktree-change-grew-past-the-budget.md` still covers them.
- Dropped *pulled* commits are not reported until `range-pull` (#563) has a ledger of pulls.

## Verification

Scoped, not whole-workspace: clippy `--all-targets -D warnings` clean over `tddy-subagent-worktree`,
`tddy-service`, `tddy-discovery`, `tddy-session-tool-client`, `tddy-tools`, `tddy-daemon-rpc`,
`tddy-session-agents`, `tddy-session-lifecycle`. The mechanics, daemon and discovery acceptance suites
drive worktrees made by #560 and are green only on top of it. Everything else is CI's.

## Backlog entries and code issues

No `docs/dev/todo/` entry was resolved by this change and none was deleted. The code-issue records for
`subagent.rs` and the session-tool-client `lib.rs` carry a dated measurement row; neither was closed.
