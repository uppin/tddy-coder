# A caller pulls a chosen range of a subagent's commits, mid-conversation or at its end - PRD

**Date**: 2026-09-30
**PRD Type**: Enhancement
**Stack**: `#agent-worktree` 4/4

## Affected Features

- **Primary Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) — new MCP tool
  `subagent_pull`; `subagent_end` takes a range

## Summary

`subagent_end` (`#agent-worktree` 1/4) hands over everything a conversation committed, once, at the
end. A caller that wants the subagent's first fix now while it keeps working, or wants only some of
its commits, has no way to say so. **`subagent_pull`** applies a chosen range of the conversation's
commits to the caller's worktree mid-conversation, and **`subagent_end`** takes the same range. The
conversation remembers what it has handed over, so nothing is applied twice.

## Proposed Changes

### What's Changing

- `subagent_pull { sessionId, from?, to? }` and `subagent_end { sessionId, from?, to? }`:
  - `from` / `to` are commit short hashes on the conversation's branch, **both inclusive**: the
    changes those commits made, in order. The conversation's base is not a pullable commit.
  - Omitted `from` is the earliest commit not yet pulled; omitted `to` is the branch tip.
  - Commits already pulled inside the range are **skipped**, not re-applied.
  - Each commit is applied as its own 3-way apply, in order, as uncommitted changes to the caller's
    worktree; conflict markers and conflicted paths as in 1/4.
- The reply lists what was applied and what was skipped:

  ```json
  { "pulled": { "commits": ["3f9c2ab", "9e01d4c"], "skipped": ["a77b310"],
                "files": { … }, "lines": { … }, "conflicts": [] } }
  ```

- **The pull ledger.** The conversation records every commit it has pulled. `subagent_end` with no
  range pulls every commit on the branch not yet pulled — which, when nothing was pulled earlier, is
  exactly 1/4's behaviour.
- **A rewind that drops pulled commits** (2/4) does not un-apply them — they are the caller's now.
  `worktreeReset` gains `droppedPulledCommits: [...]`, naming them so the caller can decide.
- Refusals: a commit not on the branch; `from` after `to`; a range with nothing left to pull is **not**
  a refusal — it replies with an empty `commits` list.
- `subagent_pull` while a turn is running is refused, like `subagent_end`.

### What's Staying the Same
- `subagent_end` without arguments: same result as 1/4 when nothing was pulled before.
- The subagent's branch and worktree are untouched by a pull.

## Impact Analysis
- `Pull` takes an explicit ordered commit list; the ledger lives with the conversation in
  `tddy-tools`; `subagent_pull` and its allowlist entry; `subagent_end`'s range.

## Acceptance Criteria
- [ ] `subagent_pull` with no range applies every unpulled commit, in order
- [ ] `from`/`to` (inclusive) apply exactly those commits
- [ ] A commit pulled once is skipped by every later pull and by `subagent_end`
- [ ] `subagent_end { from, to }` pulls the range, then deletes the worktree
- [ ] A rewind that drops pulled commits names them in `worktreeReset.droppedPulledCommits`
- [ ] A commit not on the branch, and `from` after `to`, are refused
- [ ] A pull while a turn runs is refused
- [ ] `subagent_pull` is allowlisted in the sandbox recipes

## Non-goals
- Un-applying a pulled commit from the caller's worktree.
- Persisting the ledger across a `tddy-tools` restart (the conversation does not survive one either).

## References
- [managed-codebase-subagents.md](../managed-codebase-subagents.md)
- Discovery: `docs/dev/1-WIP/2026-09-30-agent-worktree-range-pull-initial-discovery.md`
