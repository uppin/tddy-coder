# tddy-subagent-worktree

A subagent conversation's own branch and worktree. A specialized subagent that edits code used to
write its caller's worktree directly; here each conversation gets a worktree of its own inside the
session worktree, every mutating call of it is committed there, and the result is handed to the
caller as uncommitted changes — or discarded.

A **leaf crate**: git through the CLI (`std::process` via tokio), nothing from the rest of the
workspace. The daemon's exec-tool route, its `ConversationWorktree` handler, the jail session channel
handler and the subagent loop depend on it; it depends on none of them. `tddy-sandbox-runner` must
**not** depend on it — the runner relays, it never runs git, and it is inside every jail.

## Surface

| Item | What it is |
|---|---|
| `ConversationId::parse` | `[A-Za-z0-9._-]{1,64}`, no leading `.` — the only way an id becomes a path or a branch name |
| `ConversationWorktrees::{new, path_of, branch_of, existing, ensure}` | one session's conversation worktrees; `existing` never creates, `ensure` creates on first use |
| `ConversationWorktree::{commit_changes, pull_into_caller, remove}` | one commit per call that changed files; the 3-way hand-over of every subagent commit (`pull_range` over the whole branch with nothing pulled yet); deletion of worktree, branch and base ref |
| `ConversationWorktree::subagent_commits` | the subagent's own commits after the base, oldest first — the branch's first-parent line without merges, so a caller sync is never listed as subagent work. Every pull, reset and diff reads this one listing |
| `ConversationWorktree::sync_with_caller`, `SyncOutcome`, `WorktreeSync`, `SYNC_NOTICE_PATHS`, `SYNC_MERGE_SUBJECT`, `OUTSIDE_A_TOOL_CALL_SUBJECT` | merges the caller's current files (`HEAD` + uncommitted) into the branch as a merge commit before a turn: `Unchanged`, `Merged` (commit, counts, at most 20 sorted paths and a count of the rest), or `Conflicted { paths }` with nothing moved |
| `ConversationWorktree::reset_to`, `ResetTarget`, `WorktreeReset` | hard reset to the base or one of the subagent's commits, untracked files removed and ignored ones kept; reports where it stands and the subagent commits it dropped |
| `ConversationWorktree::pull_range`, `PullRange`, `RangePullOutcome` | the 3-way hand-over of a chosen inclusive range of subagent commits, one apply per commit, skipping those the caller says it already pulled |
| `ConversationWorktree::diff`, `ConversationDiff`, `DIFF_TEXT_CAP_BYTES` | the unified diff `from..to` (default base..tip), whole-range counts, text capped at 64 KiB at a line boundary, `includes_caller_changes` when the range spans a sync; read-only |
| `ToolEffect::of` | the one fail-closed read-only classifier: `Read`, `Glob`, `Grep`, `SemanticSearch`, `ReadLints` read, everything else mutates |
| `run_in_conversation` | the whole per-call rule — which root a call runs at, and the commit that follows a mutating one |
| `with_worktree_change` | merges `worktreeChange` into a tool result |
| `WorktreeChange`, `FileCounts`, `LineCounts`, `PullOutcome` | the facts reported per call and per pull |

Mechanics and the decisions behind them: [docs/conversation-worktree.md](docs/conversation-worktree.md).

## Quick Start

```bash
./test -p tddy-subagent-worktree
```

The suites run against real temporary git repositories — a git double would test nothing — and the
per-call rule runs with the real `tddy_tool_engine::execute_tool` as its executor.

The sync runs `git merge-tree --write-tree --merge-base`, so the host that owns the session worktree
needs **git ≥ 2.40**.

## Documentation

Product: [managed-codebase-subagents.md](../../docs/ft/coder/managed-codebase-subagents.md) § The conversation worktree.
