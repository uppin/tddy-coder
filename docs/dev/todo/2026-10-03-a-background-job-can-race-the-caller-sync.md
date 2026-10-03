# 2026-10-03 — A background job can race the caller sync

**Category:** Known gap — developer consented 2026-10-03
**Source:** changeset [`2026-10-03-agent-worktree-caller-sync`](../1-WIP/2026-10-03-agent-worktree-caller-sync.md)
(`/validate-changes` first pass, ⚠ warning)

Before every subagent turn, `ConversationWorktree::sync_with_caller`
(`packages/tddy-subagent-worktree/src/sync.rs`) snapshots the conversation worktree's uncommitted
files, merges the caller's current files in, and moves the branch and the checkout to the merge
(`reset --hard`). It holds the worktree's exclusive lock (`serialise::exclusive`), but a background
job the subagent started in an earlier turn (`SHELL` with `run_in_background`, collected later by
`Await`) does not take that lock — it writes the conversation worktree whenever it runs.

A job that writes between the snapshot and the `reset --hard`:

- loses that write when the file is one the merge rewrites (the reset puts the merged content
  back), or
- leaves it uncommitted beside the merge, to be committed by the next tool call or sync as
  "Changes made outside a tool call" — attributed to the wrong turn.

Nothing is corrupted in git (every step is an object write plus one ref move), but a job's output
can silently disappear.

## Why deferred

It needs a job-aware sync, and the information is in the wrong crate: the background jobs live in
`tddy-tool-engine`'s `TaskRegistry` (registered by the `Shell` tool, collected by `Await`, keyed by
session, not by conversation), held by whichever executor ran the call — the daemon's or a jail's —
while the sync runs in `tddy-subagent-worktree` (which only wraps a call with its commit,
`run.rs`) and is asked for by `tddy-discovery`'s turn through a port that sees only "sync this
conversation". Threading "does this conversation have a running job" across
`tddy-discovery` → `tddy-tools` → the daemon → `tddy-subagent-worktree` is a change to every layer
of the wire for a window that today needs a job still running when the caller sends the next turn.

## Fix

Postpone the sync while that conversation has a running background job: the daemon's `Sync` arm
asks the task registry whether any job started from this conversation is still running, and if so
answers "not synced — a background job is running" (a distinct answer, not `{"sync": null}`), which
the turn reports to the subagent as it does a conflict's alternative. Alternatively, have the job
runner take `serialise::exclusive(root)` around each write burst — cheaper, but a long-running job
would then block every turn. Either needs the task registry reachable across crates.
