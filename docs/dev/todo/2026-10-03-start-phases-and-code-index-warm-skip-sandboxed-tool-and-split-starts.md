# 2026-10-03 — Start phases and the code-index warm skip the sandboxed, tool and split starts

**Category:** Deferred from `indexing-indicators` (#571, `#live-plan` 12/15)
**Source:** [`2026-10-03-indexing-indicators.md`](../1-WIP/2026-10-03-indexing-indicators.md) — Technical Debt & Production Readiness

## What is not done

`StartPhase` events (worktree, semantic index, agent) and the background code-index warm are wired
into the **claude-cli, cursor-cli and workspace** starts only. These starts report no phases and
trigger no warm:

- the sandboxed claude-cli and cursor-cli starts;
- the tool start;
- the split starts;
- children spawned by a pr-stack orchestrator or a grill-me conversation (their handlers pass
  `AttachmentProgressSink::discarding()`).

A session of one of those kinds shows no start phase in the create pane, and its header indicator
renders nothing until a navigation request warms the index lazily.

## Why it was deferred

Each path has its own start function and its own place where "the worktree now exists" becomes true
(a jail is built after the index; a split start has a codebase half and an agent half on different
hosts). Threading the progress sink and calling `announce_worktree_ready` at the right point in each
is a per-path change with its own tests, and #571's stated Responsibility is the three session types
the create pane starts most. Widening a stacked PR across six more start paths also puts more files
in front of the nodes above and below it.

## What would close it

For each path above: report `begin_phase` / `end_phase` for the steps it actually has (a failed step
sends no END), call `DaemonSessionHost::announce_worktree_ready` once the worktree exists, and add a
`start_phase_acceptance.rs` case beside the existing two. The `TODO(indexing-indicators)` in
`svc_start_session_core.rs` points here and goes away with the last path.
