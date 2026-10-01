# oversized-file: subagent_runtime.rs

**Location:** `packages/tddy-discovery/src/subagent_runtime.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — hand-measured during `/plan-red` Step 2b for the
subagent turn-control changeset
**Metrics:** **716 production lines** (of 1016 total; `#[cfg(test)]` at `:717`) · budget 500 ·
**1.4× over**
**Thresholds breached:** length 716 > 500
**Restructure:** not required — one extraction, or it may fall under budget on its own
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-26 | 557 | first detection — 57 lines over |
| 2026-09-26 | 595 | after `DeferredTurn` took a `TurnRequest` instead of a `prompt_text`, `prompt_outcome_json` grew `messages`/`clampedMaxTurns`, and `TurnEnd::took_a_turn` was removed |
| 2026-09-27 | 716 | after the provider dimension: `PendingTurns` gained `provider` + `provider_queue_position` / `provider_queue_size` / `provider_of`, `SubagentConversation` gained `provider`, and `pending_turn_json` moved in from `tddy-tools`' `server.rs` (which lost the same ~25 lines). The provider queue itself went to a submodule, `subagent_runtime/provider_queue.rs`, rather than in here |

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

Extracting `PendingTurns` and its `TurnState` / `PendingTurn` types (now ~250 lines, both queue
dimensions included) into their own module — `subagent_runtime/pending_turns.rs`, beside the
`provider_queue` submodule the 2026-09-27 change already added — takes the parent to roughly 465
and gives the queue a home of its own. It is the part with real logic, and the part both
[`2026-09-20-subagent-turn-queue-visibility`](../../../../docs/dev/changesets/) and the provider
dimension grew. Ordinary `extract_module`, not a hard one.

Fix the `took_a_turn` premise at the same time, or before: it is a two-line change and a comment
that is currently wrong.

## Verified by hand

2026-09-26 — Read the file. Confirmed the test module boundary at `:558`. Read `TurnEnd::from` and
cross-checked against `SpecializedSubagentSession::prompt` and `run_one_turn` in
`subagent.rs:864-910`, which push `ChatMessage::assistant` and `ChatMessage::tool_result` inside the
loop — confirming the comment's premise is false. Did **not** run `restructure check`.
| 2026-09-28 | 729 | +13 in PR #556 (`#subagent-control` 4/5): the yielded outcome's `prompt_outcome_json` fields (`firedCondition`, `yieldedMessageId`) and their framing |
| 2026-10-01 | gate: 729 → 732 | **worse** by 3 in PR #561 (`#agent-worktree` 2/4): `prompt_outcome_json` adds the `worktreeReset` field. No other node of the stack touches this file, so the split was open to this PR; **deferred at the developer's direction** — three lines do not justify decomposing a 729-line file inside a stacked PR, and the extraction this record already names (the outcome-JSON rendering) is one move. Do it as its own change. |
