# A resumed subagent works on the caller's current files - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) — § The
  conversation worktree: before every turn the worktree takes in what the caller changed since the
  last one; § A rewind takes the worktree back, § `subagent_pull`, § `subagent_diff`: a caller's
  merged changes are never the subagent's work

## Summary

A subagent conversation's worktree is cut once, from the caller's files at the conversation's first
write, and afterwards moves only through the subagent's own tool calls. The normal hand-off breaks
that: the subagent finishes a turn, the caller takes some of the work, edits files itself, and
prompts or resumes the conversation — and the subagent carries on in a tree the caller no longer
has. What it writes next is a diff against content that is gone.

After this change, **every** `subagent_prompt` and `subagent_resume` first brings the conversation's
worktree up to the caller's current files — its `HEAD` plus staged, unstaged and untracked changes —
by **merging** them into the conversation's branch, so the subagent's own unpulled work survives
beside the caller's. The subagent is told which files changed under it. A merge that conflicts
refuses the turn and names the files. `syncWorktree: false` opts a turn out.

## Background

`#agent-worktree` (#560–#563) gave each conversation its own worktree, one commit per mutating call,
a rewind that takes the worktree back, and pulls and diffs over the conversation's commits. Its
model of the branch is "the base, then the subagent's commits" — every operation walks
`base..branch`. A caller-side change has no way onto that branch today.

## Proposed Changes

### What's Changing

**The sync, before every turn.**

- Runs on `subagent_prompt` and `subagent_resume`, before the turn's first model call — after a
  rewind's reset when the resume rewinds.
- Does nothing when the conversation has no worktree yet (its reads already see the caller's files),
  or when the caller's files are exactly what the last sync (or the conversation's start) took in.
- Otherwise: snapshots the caller as the conversation's start does, and records a **merge commit**
  on the conversation's branch — first parent the subagent's tip, second parent the caller snapshot —
  merged 3-way against the last caller state the conversation took in. Changes the caller already
  pulled from the subagent are identical on both sides and merge cleanly.
- Leaves the conversation worktree checked out at the merge.
- Uncommitted changes already sitting in the conversation worktree (a background job that finished
  between turns) are first committed as the subagent's own, subject `Changes made outside a tool call`.

**A conflict refuses the turn.** When the caller's changes and the subagent's unpulled work touch
the same lines, nothing is merged and the turn does not run. The refusal names every conflicted
path and says what to do: pull the subagent's commits first, change the files, or resume with
`syncWorktree: false`. A resume that rewound has already been rewound (and its worktree reset) — the
refusal leaves a consistent conversation, not the one before the call.

**The subagent is told.** When a sync merged something, one user message is appended last before the
turn runs:

> The caller changed 3 files since your last turn (+41 −7): src/lib.rs, src/new.rs, README.md. Your
> worktree now has their versions — re-read before relying on what you read earlier.

At most 20 paths, then `and N more`. It appears in the turn outcome's `messages` like any other.

**The turn outcome reports it** (absent when nothing was merged):

```json
"worktreeSync": {
  "commit": "7d1e0aa",
  "files": { "created": 1, "updated": 2, "removed": 0 },
  "lines": { "added": 41, "removed": 7 },
  "paths": ["src/lib.rs", "src/new.rs", "README.md"],
  "morePaths": 0
}
```

**`syncWorktree`** (boolean, default `true`) on `subagent_prompt` and `subagent_resume`. `false` runs
the turn on the conversation worktree as it stands.

**A caller's merged changes are never the subagent's work.** The subagent's commits are the
branch's first-parent line without merge commits:

- `subagent_pull` / `subagent_end` apply only subagent commits — never a merge, so the caller is never
  handed its own changes back;
- a rewind's reset targets subagent commits only, and `droppedCommits` names subagent commits only; a
  reset past a sync drops that sync too, and the next turn's sync takes the caller's files in again;
- `subagent_diff` takes subagent commits (or the base) as bounds; a merge commit is refused as a bound.
  The diff stays a tree diff — a range that spans a sync includes the caller's merged changes, and the
  answer says so with `"includesCallerChanges": true`.

### What's Staying the Same

- The conversation's start, the per-call commit, `worktreeChange`, the pull ledger, `subagent_end`,
  `subagent_cancel`.
- A conversation that never wrote: no worktree, no sync, no notice.
- Daemon-run conversations (still writing the session worktree — see the TODO from #560).
- The caller's own worktree, index and branch: a sync only reads them.

## Impact Analysis

### Technical Impact

- `tddy-subagent-worktree`: a sync operation (`merge-tree --write-tree --merge-base`, `commit-tree`,
  branch move, checkout) under the worktree lock; the "last caller state taken in" derived from the
  newest merge's second parent; every commit listing switched to first-parent without merges.
- Wire: a `SyncOp` (`ConversationWorktreeRequest.op` tag 15) and its Connect-JSON arm.
- `tddy-discovery`: a sync port beside the reset port; `take_turn` runs it and appends the notice;
  `PromptOutcome::worktree_sync`.
- `tddy-tools`: the sync port, `syncWorktree` on both tools, `worktreeSync` in the outcome.
- **Requires git ≥ 2.40** on the host that owns the session worktree: the sync merges with
  `git merge-tree --write-tree --merge-base <base> <ours> <theirs>`, and `--merge-base` with
  `--write-tree` arrived in git 2.40. An older git makes every sync fail with a git error, which
  refuses the turn (`syncWorktree: false` still runs it).
- The jail host bridge serves `ConversationWorktree` only for the session its jail was built for,
  and finds that session's worktree the way the token route does (under the session OS user's
  sessions base) — `Sync` reads the caller's uncommitted files, so a forged session id there would
  be a read path into another session.

### User Impact

- A main agent can hand a task off, take part of the result, fix things itself, and send the subagent
  back in — and the subagent sees the fixed files.
- One more git snapshot per turn on a conversation that has a worktree (the same cost as the
  conversation's start); a no-op when nothing changed.
- A turn can now be refused for a conflict the caller created; the refusal says how to proceed.

## Acceptance Criteria

- [ ] A prompt or resume after the caller edited a file runs the turn on a worktree holding the
      caller's edit
- [ ] The subagent's unpulled commits survive the sync
- [ ] Changes the caller already pulled merge without conflict
- [ ] Nothing is merged, and no notice is sent, when the caller's files have not changed since the
      last sync — or when the conversation has no worktree
- [ ] A conflict refuses the turn before any model call, names every conflicted path, and leaves the
      branch and worktree as they were
- [ ] The notice names the changed paths (at most 20, then a count) and is the last message before
      the turn
- [ ] `worktreeSync` reports the merge commit, counts and paths; absent when nothing merged
- [ ] `syncWorktree: false` on prompt or resume skips the sync
- [ ] A rewind's reset runs before the sync; a reset past a sync drops it and the next sync takes the
      caller's files in again
- [ ] Pulls never apply a sync merge; `droppedCommits` and reset targets are subagent commits only
- [ ] `subagent_diff` refuses a merge commit as a bound and marks a range that spans a sync
- [ ] Uncommitted changes in the conversation worktree are committed as the subagent's before a sync

## Non-goals

- Syncing daemon-run conversations.
- Syncing **during** a turn (the caller editing while a turn runs is picked up by the next turn).
- Resolving conflicts automatically.

## Decisions made while planning (for review)

| Decision | Alternative rejected |
|---|---|
| Subagent work = first-parent line without merges | Tag sync commits by subject — fragile, a subagent `SHELL git commit` could forge one |
| "Last caller state" derived from the newest merge's second parent | A `refs/tddy/subagent-synced/…` ref — needs invalidating on every reset |
| Notice appended **last**, after prompt / correction / replacement | First — a replacement must complete its tool-call group before any user message |
| `subagent_diff` stays a tree diff, flags `includesCallerChanges` | Subagent-only diff — needs replaying commits; not what `git diff a b` says |
| Dirty conversation worktree is committed as subagent work before the sync | Refuse — a finished background job would block every later turn |
| A sync refusal after a rewind leaves the rewind done | Undo the reset — needs a second reset op and can itself fail |

## References

- [managed-codebase-subagents.md](../managed-codebase-subagents.md)
- Discovery: `docs/dev/1-WIP/2026-10-03-agent-worktree-caller-sync-initial-discovery.md`
