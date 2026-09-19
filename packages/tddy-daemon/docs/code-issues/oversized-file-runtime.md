# oversized-file: runtime.rs — the daemon's composition root

**Location:** `packages/tddy-daemon/src/runtime.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 by the `/pr-wrap` file-length gate on #498
**Metrics:** **1,423 production lines** (1,420 before #498) · budget 500
**Restructure:** required
**Status:** Open — pre-existing; #498 added 3 lines and deferred with explicit developer consent

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 1,423 | first detection; 1,420 before #498 |

## What the gate found

Already 2.8× the budget before #498 touched it. #498's contribution is **three lines**: eight
`crate::relay_idle` / `crate::user_sessions_path` paths re-pointed at `tddy_session_lifecycle` when
the re-export shims were deleted, which reflowed three lines under `rustfmt`.

The file is the daemon's composition root — `runtime::build` wires roughly twenty services — so its
size is a real design question, not an accident of this change. See the companion record
`complexity-runtime-build.md`.

## What would close it

A decomposition of `runtime::build` along service-group seams. Out of scope for #498, which moved no
production module by design: its `## Boundaries` say no `src/` file moves between crates in that node.
