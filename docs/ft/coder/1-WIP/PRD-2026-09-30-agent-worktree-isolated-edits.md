# A subagent edits its own worktree, commits every change, and hands the result back at the end - PRD

**Date**: 2026-09-30
**PRD Type**: Enhancement
**Stack**: `#agent-worktree` 1/4

## Affected Features

- **Primary Feature**: [Managed-codebase subagents](../managed-codebase-subagents.md) — a subagent's
  mutating tool calls stop landing in the caller's worktree; they land in a per-conversation
  worktree, one commit per call, and come back to the caller when the conversation ends
- **Related Feature**: [Specialized subagents](../specialized-subagents.md) — § Agent definition
  format: the mutation tools a def binds now write to the conversation's worktree
- **Related Feature**: [Session agent roster](../../daemon/session-agent-roster.md) — **unchanged**:
  its non-goals *"Write-back from a remote clone"* and *"Per-agent clone isolation on one host"*
  still hold for peer-owned agents, which this PRD does not touch

## Summary

A specialized subagent that edits code today edits the **caller's own worktree**, directly, while
the caller is also working in it. Nothing records which change came from which tool call, nothing
can take a change back, and a subagent whose conversation is cancelled leaves whatever it wrote
behind.

After this change, a conversation's first mutating tool call cuts an **ephemeral branch and
worktree** from the caller's worktree — its tip **plus the caller's uncommitted changes**, which
become one commit on the new branch and nowhere else. Every mutating call that changes a file is
**committed** there, and its tool summary says what it did: how many files it created, updated and
removed, how many lines it added and removed, and the commit's short hash. A new
**`subagent_end`** hands the subagent's work to the caller as **uncommitted changes** and deletes the
worktree; **`subagent_cancel`** deletes it without handing anything back.

## Background

Subagent write tools (`WRITE`, `STR_REPLACE`, `DELETE`, `SHELL`) are Managed-access only: the
in-process loop in `tddy-tools` sends each call to the facilitating daemon, which runs it on the
session worktree. That worktree is the caller's. Three things follow:

1. **Two writers, one tree.** A subagent and its caller edit the same files concurrently, with no
   boundary between their changes.
2. **No attribution.** A tool summary says `bytesWritten`; it cannot say what changed on disk, and
   a `SHELL` call says nothing at all about the files it touched.
3. **No undo.** A cancelled or misdirected conversation's edits stay. The rewind the caller already
   has (`subagent_resume{fromMessageId}`) rewinds the *transcript* and leaves the files as they are.

Commits per call are the substrate for the rest of the `#agent-worktree` stack: resetting on a
rewind, diffing between any two points, and pulling a chosen range.

## Proposed Changes

### What's Changing

**The conversation worktree.**

- Created **lazily**, on the conversation's first **mutating** tool call. A conversation that only
  reads never creates one.
- Lives at `<session worktree>/tmp/subagent-worktrees/<conversation id>`, on branch
  `tddy/subagent/<session id>/<conversation id>`. The directory is excluded through the repository's
  `info/exclude`, so it never shows in the caller's `git status`.
- Cut from the caller worktree's `HEAD`. When the caller has uncommitted changes — staged, unstaged,
  or untracked and not ignored — they are captured as **one commit on the subagent branch**, and the
  caller's index, branch and files are left exactly as they were. That commit (or `HEAD` when the
  caller was clean) is the conversation's **base**.
- Once it exists, **every** tool call of the conversation — reads included — runs against it, so the
  subagent reads what it wrote.
- A conversation id that is not a safe path and ref component (`[A-Za-z0-9._-]`, no leading `.`,
  at most 64 characters) is refused when the worktree would be created.

**One commit per mutating call.**

- A call is **mutating** unless it is one of `READ`, `GLOB`, `GREP`, `SEMANTIC_SEARCH`,
  `READ_LINTS`. The list is fail-closed: `AWAIT` is mutating (a background `SHELL` job changes files
  while it is awaited), and so is any tool name the classifier does not know.
- After a mutating call, every change in the conversation worktree is committed as one commit whose
  subject names the tool. A mutating call that changed nothing produces **no** commit.
- The tool result's summary gains `worktreeChange` on every mutating call:

  ```json
  "worktreeChange": {
    "commit": "3f9c2ab",
    "files": { "created": 1, "updated": 2, "removed": 0 },
    "lines": { "added": 41, "removed": 7 }
  }
  ```

  `commit` is present **only** when a commit was made. A binary file counts as a file and adds no
  lines. A read-only call carries no `worktreeChange`.

**`subagent_end` — finish, hand over, delete.**

- `subagent_end { sessionId }` applies everything the conversation committed since its base to the
  caller's worktree as **uncommitted changes**, then deletes the worktree and its branch and closes
  the conversation.
- The apply is a **3-way** apply. A hunk that no longer applies because the caller has changed the
  same lines is written with **conflict markers**, and the reply names every conflicted path. The
  caller's `HEAD` never moves and no commit is made on the caller's branch.
- The reply:

  ```json
  { "ended": true,
    "pulled": { "files": { "created": 1, "updated": 3, "removed": 0 },
                "lines": { "added": 52, "removed": 9 },
                "conflicts": ["src/lib.rs"] } }
  ```

  `pulled` is `null` when the conversation never created a worktree.
- `subagent_end` on a conversation with a turn still running is refused, naming the running turn;
  the caller awaits it or cancels.

**`subagent_cancel` — discard.** Deletes the worktree and its branch; nothing reaches the caller.

### What's Staying the Same

- A read-only conversation behaves exactly as today and touches no git state.
- `Local`-access subagents stay read-only; their mutation tools still refuse.
- The caller's own tools, branch, index and history — nothing in the caller's worktree changes until
  `subagent_end`.
- Transcript ids, rewind, replacement and yield semantics. A rewind does **not** yet reset the
  worktree (`#agent-worktree` 2/4).
- Daemon-run conversations (`OpenAgentConversation` served locally, peer-owned agents) keep today's
  behaviour; see Non-goals.

## Impact Analysis

### Technical Impact

- A new crate owns the git mechanics (create, snapshot, commit, change facts, 3-way pull, remove),
  git CLI only, in the style of `tddy-session-sync` and `tddy-daemon-livekit::session_room`.
- `ExecuteToolRequest` gains `conversation_id`; the jail's `ExecuteTool` relay must carry it.
- `ExecToolService` gains a typed `ConversationWorktree` RPC (`Pull`, `Remove`), relayed out of the
  jail.
- The daemon's exec-tool route resolves the conversation worktree as the tool root and commits after
  a mutating call.
- `tddy-discovery`'s subagent loop records each tool result's commit; `tddy-tools` adds
  `subagent_end` and routes `subagent_cancel` through `Remove`.
- `tddy-sandbox-recipes` allowlists `subagent_end`.

### User Impact

- **Behaviour change**: a subagent's edits reach the caller only on `subagent_end`. A caller that
  never ends its conversations loses them when the conversation is cancelled — this is the point, and
  the tool descriptions say so.
- The caller sees per-call change facts and commits in every turn outcome.

## Implementation Plan

1. The worktree mechanics crate, tested against real repositories.
2. The wire: `conversation_id`, the `ConversationWorktree` RPC, the jail relay.
3. Daemon routing and per-call commit.
4. The subagent loop and the MCP tools.
5. Documentation of the new lifecycle in `managed-codebase-subagents.md` at wrap.

## Acceptance Criteria

- [ ] A conversation that only reads never creates a worktree or a branch
- [ ] The first mutating call creates `<session worktree>/tmp/subagent-worktrees/<conv>` on
      `tddy/subagent/<session>/<conv>`, cut from the caller's `HEAD`
- [ ] The caller's uncommitted changes, untracked files included, are one commit on the subagent
      branch; the caller's index, branch and files are unchanged
- [ ] The conversation worktree never appears in the caller's `git status`
- [ ] After creation, reads see the subagent's own writes
- [ ] Each mutating call that changed files makes exactly one commit; one that changed nothing makes
      none
- [ ] `worktreeChange` reports created / updated / removed files, added / removed lines, and the
      commit's short hash when one was made; a read carries none
- [ ] `AWAIT` and unknown tools are treated as mutating
- [ ] An unsafe conversation id is refused before any git state is created
- [ ] `subagent_end` applies base..tip to the caller's worktree as uncommitted changes, 3-way, with
      conflict markers and the conflicted paths reported; the caller's `HEAD` does not move
- [ ] `subagent_end` deletes the worktree and branch and closes the conversation; it is refused while
      a turn runs
- [ ] `subagent_cancel` deletes the worktree and branch; the caller's worktree is unchanged
- [ ] `subagent_end` is allowlisted in the sandbox recipes

## Non-goals

- **Daemon-run conversations.** Conversations whose loop the daemon runs — `open_local`,
  `open_owned`, a peer's `RemoteAgentSession` — keep writing to the session worktree (or proxying to
  it). Recorded as a TODO.
- **Orphan sweep.** A `tddy-tools` process that dies without ending or cancelling leaves its worktree
  and branch. The directory goes with the session worktree; the branch does not. Recorded as a TODO.
- **Resetting on rewind, diffing, range pulls** — `#agent-worktree` 2/4, 3/4 and 4/4.

## References

### Affected Features (Complete List)
- [managed-codebase-subagents.md](../managed-codebase-subagents.md) — the conversation worktree, per-call commits, `subagent_end`
- [specialized-subagents.md](../specialized-subagents.md) — mutation tools write to the conversation worktree
- [session-agent-roster.md](../../daemon/session-agent-roster.md) — unchanged; non-goals restated

### Related Documentation
- Discovery: `docs/dev/1-WIP/2026-09-30-agent-worktree-isolated-edits-initial-discovery.md`
