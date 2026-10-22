# oversized-file: spawn.rs

**Location:** `packages/tddy-sandbox-app/src/spawn.rs`
**Category:** oversized-file
**Detected:** 2026-10-22 by `/pr-wrap` step 3.5 file-length gate on PR #670 (`#provider-route` 9/9)
**Metrics:** **823 production lines** (counted to the first `#[cfg(test)]`) · budget 500
**Thresholds breached:** length 823 > 500
**Restructure:** required — not designed, `/code-restructuring` territory
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-22 | 823 | first record; 788 at PR #670's merge-base (`655970c72`), already over budget. +35 in #670: `AgentKind::{OpenCode, Muse}`, their parsing, and `unstarted_agent_kind` (the refusal for both, see `docs/dev/todo/2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md`). |

## What the tool found

The awk count to the first `#[cfg(test)]`, the one `/pr-wrap` step 3.5 uses. Complexity, nesting and
CRAP were not measured (no `analyze coverage` run: the disk on the measuring host was too tight for an
instrumented build).

## Why it matters here

The file was over budget before #670 and is a little further over now. Every PR that adds a few lines
here looks fine in review, which is how a file gets this size.

## What would close it

The agent-kind surface (`AgentKind`, its parsing, the refusal) is a self-contained seam of about 45 lines; the larger seam is `spawn_claude_sandbox` (~300 lines) against the env and replacement builders above it. Plan both with `restructure check --deep`.

## Verified by hand

2026-10-22 — Confirmed both counts with the gate's awk, and read #670's diff of the file. Not
decomposed in #670: the orchestrator scoped that PR's decomposition to files it pushed past 500.
