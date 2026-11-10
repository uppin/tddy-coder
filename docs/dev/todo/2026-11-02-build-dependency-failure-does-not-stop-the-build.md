# 2026-11-02 — A failing action in a dependency target does not stop the build

**Category:** Pre-existing bug, found during `#agent-tools` 4/12 (#793)
**Source:** `/green` on `feature/agent-tools/build-tools`

## What is wrong

`tddy_build::executor::execute_target` (and `execute_target_with_tools`) runs every target in
`build_order` but keeps only the requested target's outcomes; a dependency whose action exits
non-zero is discarded, and the dependent target runs anyway against missing or stale outputs.

## Why it was not fixed here

It predates #793, and none of #793's tests rely on it. Fixing it changes the result every caller of
`execute_target` sees.

## What would close it

Stop at the first dependency target with a non-zero action and report it (target and action id),
with an acceptance test where a failing dependency prevents its dependent from running.
