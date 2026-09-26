# complexity: take_a_turn

**Location:** `packages/tddy-session-agents/src/service.rs` — `SessionAgentServiceImpl::take_a_turn`
**Category:** complexity
**Detected:** 2026-09-26 by `/analyze-clean-code` during `/pr-wrap` on PR #545
**Metrics:** **120 lines raw · 93 code lines · control nesting depth 5**
**Thresholds breached:** length 120 > 60; nesting 5 > 4
**Restructure:** `extract_method` — ordinary work, no `/code-restructuring` needed
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Code lines | Nesting | Note |
|---|---|---|---|---|
| 2026-09-26 | 120 | 93 | 5 | first detection, in PR #545 |

## What the tool found

The worst single unit in PR #545, breaching both thresholds at once. The nesting path is
fn body → `tokio::spawn` closure → `match outcome` → arm → `for frame in frames` →
`if tx.send(..).is_err()`.

It is **mostly inherited**: the previous `prompt_agent_conversation` was ~114 lines with the same
shape. But PR #545 is what lifted it into a named, shared function for two RPCs and grew it by six
lines — which was the moment to cut it, and the moment that was missed.

## Why it matters here

This is the body every conversation turn passes through, local or peer-forwarded, prompt or
resume. At depth 5 the `tx.send` failure path — what happens when a caller hangs up mid-turn — is
four levels in from the signature.

## What would close it

Three seams, none needing new state:

- the remote branch → `forward_turn_to_owner(&self, turn, &session_dir, &agent_id)`
- the spawned body → a free `async fn stream_one_turn(session, closed, requested, tx, conversation_id, turn_ended)`, which **on its own drops nesting from 5 to 3**
- the `answered (N chars)` summary → a one-liner on `PromptOutcome`

What remains is roughly 35 lines of "resolve, stamp, route".

## Verified by hand

2026-09-26 — Counted from the diff and confirmed against master that ~114 of the 120 lines predate
PR #545. Confirmed the three seams are contiguous and reference no state outside their own
arguments.
