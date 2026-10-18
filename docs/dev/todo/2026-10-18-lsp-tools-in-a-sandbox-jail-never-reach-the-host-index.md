# 2026-10-18 — Agent code tools inside a sandbox jail never reach the host's code index

**Category:** Deferred from `code-index-warm-on-demand`
**Source:** `code-index-warm-on-demand` changeset — Technical Debt & Production Readiness

## What is left

An agent's `Lsp*` / `restructure_*` calls warm the worktree's index on the host and show up live in the
session header and any open Code pane — **for sessions whose tools run on the host**. A sandboxed
session's agent runs its tools inside the jail, where the runner does not register the index-backed
executor (`tddy_lsp_executor::index_backed`): those calls never reach the host's `tddy-index-daemon`,
so they do not warm the worktree's index, their load is invisible to the header and the Code pane, and
the "index still loading" answer (`"index": "loading"` + `note`) never reaches that agent.

The Code pane still warms a sandboxed session's worktree when it is opened, so the operator-facing half
works; it is the agent half that is missing.

## Why it was deferred

The change's scope was the host side: per-root progress, the Code pane's `CodeSession`, and the
`Lsp*` loading answer on the host executor. Routing in-jail tool calls to the host index is a sandbox
bridge change (an RPC from the runner to the host, authorised to the session's worktree) with its own
jail tests, and was explicitly out of scope.

## What would close it

Give the in-jail runner an `LspExecutor` (and `RestructureExecutor`) that forwards to the host over the
sandbox bridge, which answers through the host's index-backed executor bound to the session's worktree;
then a jail suite proving a sandboxed agent's `LspDefinition` on a cold worktree is answered `loading`
and that the header shows the load. Related:
[2026-10-04-session-restructure-tools-no-jail-end-to-end-test.md](2026-10-04-session-restructure-tools-no-jail-end-to-end-test.md).
