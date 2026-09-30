# A caller can read the diff between any two points of a subagent conversation - PRD

**Date**: 2026-09-30
**PRD Type**: Enhancement
**Stack**: `#agent-worktree` 3/4

## Affected Features

- **Primary Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) — new MCP tool
  `subagent_diff`

## Summary

A turn outcome says, per tool call, how many files and lines changed and which commit recorded it
(`#agent-worktree` 1/4). It does not show the change. **`subagent_diff`** returns the unified diff
between any two commits of a conversation — by default, everything since the base.

## Proposed Changes

### What's Changing

- `subagent_diff { sessionId, from?, to? }`:
  - `from` / `to` are commit short hashes from the conversation — its base, or any
    `worktreeChange.commit` still on its branch. Omitted `from` is the base; omitted `to` is the
    branch tip.
  - The diff is `from..to`: the changes made **after** `from`, up to and including `to`.
  - The reply:

    ```json
    { "from": "a01b2c3", "to": "3f9c2ab",
      "files": { "created": 1, "updated": 2, "removed": 0 },
      "lines": { "added": 41, "removed": 7 },
      "diff": "diff --git a/src/lib.rs b/src/lib.rs\n…",
      "truncated": false }
    ```

  - The diff text is capped at 64 KiB; past the cap it is cut at a line boundary and
    `truncated: true`. The counts always describe the whole range.
- Refusals, each naming what was wrong:
  - a commit that is not in the conversation — including one a rewind dropped from the branch;
  - `from` that is not an ancestor of `to`;
  - a conversation with no worktree (nothing has been committed yet).
- A binary file appears as git's `Binary files … differ` line.

### What's Staying the Same
- Nothing is written anywhere; `subagent_diff` is read-only, and allowed while a turn runs.

## Impact Analysis
- The `Diff` operation on `ConversationWorktree`; the `subagent_diff` tool and its allowlist entry.

## Acceptance Criteria
- [ ] With no arguments, the diff is base..tip
- [ ] `from`/`to` select exactly the changes after `from` through `to`
- [ ] Counts match the diff and describe the whole range when the text is truncated
- [ ] A commit outside the conversation, a dropped commit, and a non-ancestor `from` are refused
- [ ] A conversation with no worktree is refused
- [ ] The diff is readable while a turn is running
- [ ] `subagent_diff` is allowlisted in the sandbox recipes

## Non-goals
- Diffing against the caller's worktree.
- Path filters.

## References
- [managed-codebase-subagents.md](../managed-codebase-subagents.md)
- Discovery: `docs/dev/1-WIP/2026-09-30-agent-worktree-diff-initial-discovery.md`
