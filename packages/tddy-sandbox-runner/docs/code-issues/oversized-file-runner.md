# oversized-file: runner.rs

**Location:** `packages/tddy-sandbox-runner/src/runner.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — `/pr-wrap` step 3.5 file-length gate on PR #545
**Metrics:** **2,611 production lines** (counted to the module-level `#[cfg(test)]`) · budget 500 ·
**5.2× over**
**Thresholds breached:** length 2,611 > 500
**Restructure:** required — `extract_module --to_file`, `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-26 | 2,610 | first detection, at PR #545's merge-base |
| 2026-09-26 | 2,611 | +1 in PR #545 — **two prose comments only** ("the five …" → "the six …"), no code. Recorded because the gate attributes growth whatever its size, and because a one-line delta is the clearest possible evidence that this file's size predates that change |

## What the tool found

`wc -l` to the module-level `#[cfg(test)]`. Not measured: complexity, nesting, CRAP — this package
had no `docs/code-issues/` directory before this record, so it had never been analyzed.

Its sibling `host_relay.rs` already carries a complexity record
(`complexity-host-relay-run-host-relay-inner.md`, 263 lines / nesting 12), so the crate's two
largest files are now both on the books.

## Why it matters here

This is the in-jail runner: it holds the tool-IPC server, the relay allowlist consumption, the
egress shim, the secret-env resolution and the inner-agent spawn. It is the process every
sandboxed session's tools run inside, and it is security-relevant code — 2,611 lines is more than
a reviewer can hold while checking a confinement change.

## What would close it

Not yet designed. The plausible seams are the egress shim, the secret-env resolution and the
inner-agent spawn, each of which looks self-contained from the outside. Measure before planning:
this record exists to put the number on the table, not to prescribe a split.

**PR #545 grew it by one line of comment**, which is not the change that should pay for the
decomposition.

## Verified by hand

2026-09-26 — Confirmed the +1 is comment-only by reading the diff. Confirmed the count is to the
module-level `#[cfg(test)]`. No seam analysis performed.
