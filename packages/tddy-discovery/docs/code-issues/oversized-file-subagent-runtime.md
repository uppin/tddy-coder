# oversized-file: subagent_runtime.rs

**Location:** `packages/tddy-discovery/src/subagent_runtime.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — hand-measured during `/plan-red` Step 2b for the
subagent turn-control changeset
**Metrics:** **595 production lines** (of 807 total; `#[cfg(test)]` at `:558`) · budget 500 ·
**1.1× over**
**Thresholds breached:** length 595 > 500
**Restructure:** not required — one extraction, or it may fall under budget on its own
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-26 | 557 | first detection — 57 lines over |
| 2026-09-26 | 595 | after `DeferredTurn` took a `TurnRequest` instead of a `prompt_text`, `prompt_outcome_json` grew `messages`/`clampedMaxTurns`, and `TurnEnd::took_a_turn` was removed |

## What the tool found

Hand measurement: 557 production lines before the test module at `:558`. Marginal — 11% over.

**Not measured:** complexity, nesting, CRAP. This package had no `docs/code-issues/` directory
before 2026-09-26, so it had never been analyzed.

## Why it matters here

Marginal on length, and recorded mainly so the next planner sees it was weighed. It holds the
conversation table and the turn queue: `SubagentConversation` `:44-83`, `SubagentConversations`
`:89-113`, the `subagent_sessions()` process-wide singleton `:116-132`, `PendingTurns` `:139-311`,
and the turn runner `DeferredTurn` / `run_turn` / `TurnEnd` `:432-522`.

One substantive defect lives here independent of the length. `TurnEnd::from`'s `Err` arm sets
`took_a_turn: false` on the stated grounds that a failure *"spent tokens but added nothing to the
history"* (`:495-497`). That is not true: `SpecializedSubagentSession` pushes messages as its loop
runs, so a prompt that fails on its tenth turn has already grown the history by twenty messages.
Today the consequence is only a miscounted `conv.turns`. It stops being cosmetic as soon as a
failed prompt has to be resumable.

## What would close it

Extracting `PendingTurns` and its `TurnState` / `PendingTurn` types (~170 lines, `:139-311`) into
their own module takes the parent to roughly 390 and gives the queue a home of its own — it is the
part with real logic and the part
[`2026-09-20-subagent-turn-queue-visibility`](../../../../docs/dev/changesets/) most recently grew.
Ordinary `extract_module`, not a hard one.

Fix the `took_a_turn` premise at the same time, or before: it is a two-line change and a comment
that is currently wrong.

## Verified by hand

2026-09-26 — Read the file. Confirmed the test module boundary at `:558`. Read `TurnEnd::from` and
cross-checked against `SpecializedSubagentSession::prompt` and `run_one_turn` in
`subagent.rs:864-910`, which push `ChatMessage::assistant` and `ChatMessage::tool_result` inside the
loop — confirming the comment's premise is false. Did **not** run `restructure check`.
