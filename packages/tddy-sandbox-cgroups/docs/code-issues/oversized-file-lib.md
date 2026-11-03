# oversized-file: lib.rs

**Location:** `packages/tddy-sandbox-cgroups/src/lib.rs`
**Category:** oversized-file
**Detected:** 2026-11-03 by `/pr-wrap` step 3.5 file-length gate on PR #799 (`#agent-tools` 10/12)
**Metrics:** **756 production lines** (2026-11-03, after #799) (counted to the first `#[cfg(test)]`) · budget 500
**Thresholds breached:** length 756 > 500
**Restructure:** required. Not designed; this is `/code-restructuring` work.
**Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-03) — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-03 | 756 | first record. It was 746 at PR #799's merge-base (`f649e3702`), already over budget. PR #799 added 10: `stdio_mode_of(plan)`, which reads `SandboxPlan.stdio` where the backend used to sniff `--stdio` from the argv, plus the `TODO(jail-agnostic)` note that `plan.context_dir` adds no bind here. Not split: the developer deferred every restructure in `#agent-tools` to the sweep node |

## What the tool found

The count is the awk count up to the first `#[cfg(test)]`, the one `/pr-wrap` step 3.5 uses. The file holds the whole Linux backend in one module: cgroup delegation, namespace and mount setup, and the spawn with its stdio transport. Complexity, nesting and CRAP were not measured. `analyze coverage` was not run because the disk on the measuring host was too tight for an instrumented build.

## Why it matters here

The file was over budget before #799. Every `#agent-tools` node that changes how a jail starts edits it.

## What would close it

Not designed. The likely seams are the mount/bind assembly and the cgroup v2 delegation, each a module of its own.

## Verified by hand

2026-11-03: confirmed both counts with the gate's awk (`git show f649e3702:packages/tddy-sandbox-cgroups/src/lib.rs`), and read #799's diff of the file.
