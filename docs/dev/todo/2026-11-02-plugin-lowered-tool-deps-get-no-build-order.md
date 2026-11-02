# 2026-11-02 — Plugin-lowered tool dependencies get no producer ordering

**Category:** Deferred from `#agent-tools` 4/12 (#793)
**Source:** `/validate-changes` on `feature/agent-tools/build-tools`

## What is missing

`tddy_build::graph` adds a build-order edge from an action to the target that produces a tool's
binary (`binary.target`) only for actions declared explicitly in `BUILD.yaml`. `tool_dep_ids` on
actions a `BuildPlugin` lowers are not seen when edges are computed, so such an action can run before
its tool's producing target has built.

## Why it was deferred

No in-repo plugin lowers actions with `tool_dep_ids` naming a target-built tool, and #793's
Responsibility names explicit actions. Computing edges after lowering changes when plugins run.

## What would close it

Derive tool edges from the lowered actions (or have lowering report its tool deps) and add a graph
test with a plugin action listing a target-built tool.
