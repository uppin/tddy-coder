# oversized-file: builder.rs

**Location:** `packages/tddy-sandbox/src/builder.rs`
**Category:** oversized-file
**Detected:** 2026-10-22 by `/pr-wrap` step 3.5 file-length gate on PR #670 (`#provider-route` 9/9)
**Metrics:** **573 production lines** (counted to the first `#[cfg(test)]`) · budget 500
**Thresholds breached:** length 573 > 500
**Restructure:** required — not designed, `/code-restructuring` territory
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-22 | 573 | first record; 562 at PR #670's merge-base (`655970c72`), already over budget. +11 in #670: a hand-written `Debug` for `SecretSource`, so a literal secret in a plan never reaches a log line. |

## What the tool found

The awk count to the first `#[cfg(test)]`, the one `/pr-wrap` step 3.5 uses. Complexity, nesting and
CRAP were not measured (no `analyze coverage` run: the disk on the measuring host was too tight for an
instrumented build).

## Why it matters here

The file was over budget before #670 and is a little further over now. Every PR that adds a few lines
here looks fine in review, which is how a file gets this size.

## What would close it

The policy and env spec types (`PolicySpec`, `EnvSpec`, `SecretSpec`, `SecretSource`) are a data-only seam that would take the file under budget on its own.

## Verified by hand

2026-10-22 — Confirmed both counts with the gate's awk, and read #670's diff of the file. Not
decomposed in #670: the orchestrator scoped that PR's decomposition to files it pushed past 500.
