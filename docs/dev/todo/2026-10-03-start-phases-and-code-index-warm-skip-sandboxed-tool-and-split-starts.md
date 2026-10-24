# 2026-10-03 — Start phases skip the sandboxed, tool and split starts

**Category:** Deferred from `indexing-indicators` (#571, `#live-plan` 12/15)
**Source:** `2026-10-03-indexing-indicators` changeset — Technical Debt & Production Readiness
**Narrowed:** 2026-10-18 by `code-index-warm-on-demand` — the **code-index warm** half no longer matters
and was removed from this entry: a worktree's index warms when its Code pane opens or its agent first
calls a code tool, and the session header follows the worktree's index (`code_index.WatchRoot`), not a
per-session record, so every kind of session shows its load whoever started it. What remains is the
**start-phase** half.
**Narrowed:** 2026-10-24 by `#start-progress` 4/8 (#737) — the **split starts** report their phases now:
the worktree step relayed from the codebase host, the agent step (`STEP_AGENT`) from the agent host. (The filename keeps its original slug because
`packages/tddy-agent-launch/src/svc_start_session_core.rs` names it.)

## What is not done

`StartPhase` events (worktree, semantic index, agent) are wired into the **claude-cli, cursor-cli and
workspace** starts only. These starts report no phases:

- the sandboxed claude-cli and cursor-cli starts;
- the tool start;
- children spawned by a pr-stack orchestrator or a grill-me conversation (their handlers pass
  `AttachmentProgressSink::discarding()`).

A session of one of those kinds shows no start phase in the create pane — the Create button stays
disabled with no step text until the start ends. These starts also do not call
`DaemonSessionHost::announce_worktree_ready`, so they get no background warm at start; that is now only a
head start, since the Code pane and the agent's code tools warm on demand.

## Why it was deferred

Each path has its own start function and its own place where "the worktree now exists" becomes true
(a jail is built after the index; a split start has a codebase half and an agent half on different
hosts). Threading the progress sink at the right point in each is a per-path change with its own tests,
and #571's stated Responsibility is the three session types the create pane starts most. Widening a
stacked PR across six more start paths also puts more files in front of the nodes above and below it.

## What would close it

For each path above: report `begin_phase` / `end_phase` for the steps it actually has (a failed step
sends no END), optionally call `DaemonSessionHost::announce_worktree_ready` once the worktree exists (a
background warm head start), and add a `start_phase_acceptance.rs` case beside the existing two. The
`TODO(indexing-indicators)` in `svc_start_session_core.rs` points here and goes away with the last path.
