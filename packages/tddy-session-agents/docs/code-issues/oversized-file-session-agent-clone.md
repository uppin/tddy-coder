# oversized-file: session_agent_clone.rs

**Location:** `packages/tddy-session-agents/src/session_agent_clone.rs`
**Category:** oversized-file
**Detected:** 2026-10-01 — `/validate-changes` on PR #560 (`#agent-worktree` 1/4); the file is not edited by that PR (its `master` count is 1,157)
**Metrics:** **1,158 production lines** (counted to the first `#[cfg(test)]`; 1,257 total) · budget 500 · **2.3× over**
**Thresholds breached:** length 1,158 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-01 | 1,158 | first record. Recorded because PR #560's boundaries ("no peer-clone changes") put this file next to the change, so a later node that does touch it should know the size it starts from |

## What the tool found

`wc -l` to the first `#[cfg(test)]`. Not measured: complexity, nesting, CRAP. The module is the
clone model for a remote agent's checkout — one per (session, owning daemon) — and holds the clone
lifecycle, readiness and the hosted-clone path together.

## What would close it

Not designed. The lifecycle (create / ready / retire) versus the hosted-clone serving path is the
first reading of a seam; prove it with `restructure check --deep` before applying. Not touched by
PR #560, so no deferral decision is attached to this change.
