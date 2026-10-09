# 2026-10-09 — a cross-crate move gives the destination a dependency on a crate that only re-exports what the moved code names

**Category:** Restructure engine limit (manifest edges of cross-crate moves)
**Source:** #reshape 8/19 (`feature/reshape/move-grouped-use`). Carries the last open item of
`2026-10-08-hand-split-grouped-use-lines-before-the-agents-cluster-move.md` (R6, "a third hand edit"), deleted at that node's wrap.

## What happened

Before the `#carve` 21/21 R6 move (`tddy-session-agents`), `connection_service/agent_host_callbacks.rs:30` wrote
`use tddy_sandbox_runner::ExecuteToolResponse;`. `tddy_sandbox_runner` only re-exports that type
(`tddy-sandbox-runner/src/lib.rs:28`, from `tddy_service::proto::exec_tools`). The engine gave `tddy-session-agents` a
dependency on `tddy-sandbox-runner`, an edge the developer had not approved (D7-A). It was hand-edited to
`use tddy_service::proto::exec_tools::ExecuteToolResponse;` before the move.

## Why the engine does it

`crate_move/reexports.rs` `followed` deliberately follows only paths rooted in the **origin** crate; a path written with another
crate's name is "left alone: its meaning does not change when a file leaves the origin". `repoint_facade_imports` documents the
same limit ("a path through another crate's re-export is not followed").

## Why deferred

Re-pointing it rewrites a path the author wrote explicitly through a dependency, which is a policy change rather than a bug. It
needs a decision: always prefer the defining crate, prefer it only when the destination does not already depend on the
re-exporting crate, or only report it in `check --deep`. Mechanically it is a wrapper over `Walk::follow_absolute` starting at the
named crate (it already walks path dependencies).

## What would close it

A decided rule, implemented for the moves' manifest pass (and, if chosen, for `repoint_facade_imports`), with `check --deep` naming
every path it re-points or every edge it adds through a re-exporting crate.
