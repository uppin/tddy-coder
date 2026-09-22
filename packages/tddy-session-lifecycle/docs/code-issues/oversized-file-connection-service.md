# oversized-file: connection_service.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518; **missed by the file-length gate**
**Metrics:** ~**1,930 production lines** of 1,949 total · budget 500 · **3.9× over**
**Thresholds breached:** length ~1930 > 500
**Restructure:** `extract_module --to_file` — seam not yet designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ the automated count is **wrong for this file** — see below

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | ~1930 | first detection; +87 in PR #518 |
| 2026-09-22 | ~1931 | unchanged in substance — total 1,944 → 1,945; #494 (`#carve` 8/11) swapped the `telegram` field for `presenter_event_sink` and its doc comment, +1 line net |

## The number has to be taken by hand

`/pr-wrap` step 3.5 counts production lines to the **first** `#[cfg(test)]`. This file puts
`#[cfg(test)] use` declarations at **line 44**, for imports only its extracted test modules need, so
the automated count exits there and reports **43**. The real test module starts at `:1931`.

That is why this file has no earlier record despite being one of the largest in the crate. See
[`docs/dev/todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md`](../../../../docs/dev/todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md).

## What the file holds

A facade plus the service struct: `MpscResultStream` (`:67-120`), `DaemonSessionHost` (`:130`), and
**20+ `mod` declarations** wiring in `connection_service/`'s submodules. Most behaviour already
lives in those submodules; what remains here is the struct's fields, its constructor, and the
placement vocabulary (`CodebasePlacement`, `PlacementRequest`, `classify_placement`,
`classify_codebase_placement`, `~:1002-1125`).

## What would close it

Two seams suggest themselves and neither is proven:

- **The placement vocabulary** — `CodebasePlacement`, `PlacementRequest`, `classify_placement`,
  `classify_codebase_placement` and their tests are a cohesive ~120 lines with no dependency on
  `DaemonSessionHost`'s fields. `extract_module --to_file` as `codebase_placement.rs`.
- **`MpscResultStream`** — a general stream adapter that names nothing in this crate.

The residue is `DaemonSessionHost`'s field list and constructor, which is what a composition root
looks like and may be the right size once the rest leaves.
