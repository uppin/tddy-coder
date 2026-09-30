# Going back in a subagent conversation takes its worktree back too - PRD

**Date**: 2026-09-30
**PRD Type**: Enhancement
**Stack**: `#agent-worktree` 2/4

## Affected Features

- **Primary Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) —
  § `subagent_resume`: a rewind resets the conversation worktree to the rewind point's commit

## Summary

`subagent_resume { fromMessageId }` rewinds a conversation's transcript to a message and carries on
from there. Its files do not follow: the edits the dropped messages made stay in the worktree, so the
resumed subagent reads a tree its own history no longer explains. After this change a rewind also
**resets the conversation worktree** to the commit the rewind point had reached, and a caller that
wants to keep the files passes **`resetWorktree: false`**.

## Background

`#agent-worktree` 1/4 gives every mutating tool call its own commit and records it on the transcript
entry. That is exactly the information a rewind needs to take the files back with the transcript.

## Proposed Changes

### What's Changing

- The **reset target** is the commit of the **last entry the rewind keeps** that made one — the cut
  already extends past the named message over the tool results answering it, so a call's commit is
  kept with its call. When no kept entry made a commit, the target is the conversation's base.
- The worktree is reset **hard** to the target, untracked files removed; its branch points at the
  target. Commits past it are dropped from the branch.
- `resetWorktree` (boolean, default `true`) on `subagent_resume` opts out. With `false` the worktree
  and branch are left as they are, and later commits build on them.
- The turn outcome reports the reset:

  ```json
  "worktreeReset": { "to": "3f9c2ab", "droppedCommits": 2 }
  ```

  Absent when nothing was reset — no rewind, `resetWorktree: false`, or no worktree yet.
- A rewind on a conversation with no worktree resets nothing and creates nothing.
- The reset happens **before** the resumed turn's first model call, so the subagent's first read sees
  the reset tree.
- A failed reset fails the resume **before** the transcript is rewound, leaving the conversation as it
  was.

### What's Staying the Same

- Transcript rewind semantics, ids, and the refusal of an unknown `fromMessageId`.
- A **replacement** (the caller's substitute call and result) is never dispatched, so it makes no
  commit and has no files; resetting to a point after a replacement resets to the last real commit.
- Nothing reaches the caller's worktree; `subagent_end` still hands over what the branch holds.

## Impact Analysis

### Technical Impact
- The `Reset` operation on `ConversationWorktree`.
- `TurnRequest` gains the reset choice; `take_turn` resets between validation and the rewind.
- `subagent_resume`'s schema gains `resetWorktree`.

### User Impact
- A caller correcting a subagent mid-conversation gets a consistent tree by default. A caller that
  wants the subagent to see its earlier edits while re-reasoning passes `resetWorktree: false`.

## Acceptance Criteria

- [ ] A rewind resets the worktree to the commit of the last kept entry that made one
- [ ] A rewind to before any commit resets to the conversation base
- [ ] Untracked files the dropped calls created are removed
- [ ] `resetWorktree: false` leaves files and branch untouched
- [ ] The outcome reports `worktreeReset { to, droppedCommits }`; absent when nothing was reset
- [ ] A rewind without a worktree creates none
- [ ] The resumed turn's first tool call reads the reset tree
- [ ] A reset failure leaves the transcript un-rewound and the resume refused

## Non-goals
- Resetting daemon-run conversations (TODO from 1/4).
- Un-applying anything already handed to the caller (see 4/4).

## References
- [managed-codebase-subagents.md](../managed-codebase-subagents.md) — § `subagent_resume`
- Discovery: `docs/dev/1-WIP/2026-09-30-agent-worktree-rewind-reset-initial-discovery.md`
