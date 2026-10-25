# complexity: take_turn

**Location:** `packages/tddy-coding-subagent/src/session.rs` — `CodingAgentSession::take_turn` (the `SubagentSession` impl)
**Category:** complexity
**Detected:** 2026-10-25 by `/pr-wrap` on PR #727 (`#coagents` 8/8), hand-measured by brace matching
**Metrics:** **71 lines raw · brace depth 5**
**Thresholds breached:** length 71 > 60; nesting 5 > 4
**Restructure:** `extract_method` — ordinary work, no `/code-restructuring` needed
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Brace depth | Note |
|---|---|---|---|
| 2026-10-25 | 39 → 71 | 3 → 5 | first detection — PR #727 grew it: the main agent's answer is delivered before the in-flight slot is taken and refused at once when a deferred turn holds the slot, a deferral is recorded on the stream, and the answer is recorded by the turn it continues. Deferred to the `#coagents` sweep node by the developer (2026-10-25) |
| 2026-10-25 | 71 → 78 | 5 → 5 | **worse** by 7 in PR #776 (`#coagents` 9/9): a new prompt after a cancel-ended turn closes that turn into the history (`close_cancelled_turn`) and begins the next. Not split: the developer deferred every code issue of the `#coagents` stack past its sweep node too (2026-10-25), so this is left for after the stack lands |

## What the tool found

Three decisions in one body: which slot to take (wait, or refuse when a held turn is the one
waiting), what to do with the prompt already in flight (resume it, record a deferral, or refuse and
put it back), and how the turn settled (paused or ended). The deepest path is fn body → `match
slot.take()` → `Some(flight)` → `if answered.is_none()` → `match self.resume(..)`.

## Why it matters here

This is every turn of a coding subagent. The refusal that puts the in-flight prompt back into the slot
(`*slot = Some(flight)`) sits four levels in; a change that returns early above it loses the prompt.

## What would close it

- the slot choice → `fn claim_slot(&self, answered: Option<&str>) -> Result<MutexGuard<..>, SubagentError>`
- the `Some(flight)` arm → `fn continue_in_flight(&self, slot, flight, request) -> Result<InFlightPrompt, SubagentError>`
- the `Settled` match → `fn outcome_of(&mut self, slot, flight, settled) -> Result<PromptOutcome, SubagentError>`

What remains is about 20 lines.

## Verified by hand

2026-10-25 — Counted at the rebased tip of PR #727 and at its base (#726, `518110d17`, 39 lines,
depth 3). The three seams share only `slot`, `flight` and `answered`.
