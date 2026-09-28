# 2026-09-28 — A caller's condition on a tool call yields the subagent's turn back to it

**Type:** Feature

`#subagent-control` 4/5, [#556](https://github.com/uppin/tddy-coder/pull/556) (base:
`feature/subagent-control/agent-usage-notes`, node 3). Consumes node 1's (`#553`) result-summary
vocabulary — the stack's real dependency edge. Successor: `resume-replacement` (#557), which
consumes the yielded tool message's id.

Packages: `tddy-discovery` (predicates, the loop check, the stop reason), `tddy-session-agents`
(request parsing, chunk framing), `tddy-service` (proto fields), `tddy-tools` (MCP schemas).

## What was delivered

`subagent_prompt` and `subagent_resume` accept **`yieldConditions`** — up to 8 per turn request,
each naming a tool and what to watch for on its call:

- **`outcome`** — a fact compared for equality against the call's result summary, in the
  vocabulary node 1 extracts: `{matchedLines: 0}`, `{exitCode: 1}`, `{matchCount: 0}`,
  `{charsRead: n}`, `{bytesWritten: n}`, `{pathCount: n}`, `{error: true|false}`. The summary is
  absent exactly when the dispatch produced nothing, and that is the only thing `error: true`
  fires on.
- **`argument`** — a string field of the call's arguments containing a bounded substring (≤256
  chars); missing or non-string fields never fire.

When a condition matches, the turn stops **at that call**: the result just appended stays in the
transcript, the model is never sent it, no further model turn is spent, and the outcome returns
`stopReason: "yieldedToCaller"` naming the fired condition and the tool message's id — the anchor
node 5's resume-with-replacement consumes. Conditions are per-turn-request state, evaluated after
each call runs, on the call as made.

**Validation before any model turn**, each rejection naming the offending condition: an unknown
tool (the nine the summaries know), a fact that tool's summary cannot carry (a `matchedLines`
condition on a `WRITE` is a refusal, never a silent never-fire), an over-long needle, too many
conditions. The session-agents request parser refuses an unparseable `yieldConditions` payload
with `Status::invalid_argument` before any turn is stamped — its documented behaviour, which the
contract commit had left unimplemented (silently dropping the conditions).

The fired condition echoes on the final LiveKit frame, bounded (one condition, needle ≤256),
accounted in
`docs/dev/todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`
alongside node 1's summary bytes — the frame-budget defect itself stays open.

## Code-issue measurements at wrap

- `roster/conversation.rs` **crossed the 500 budget here** (488 → 512, +24) — decomposition
  **deferred by explicit developer consent** (node 5 threads the same framing surfaces); first
  record: `packages/tddy-discovery/docs/code-issues/oversized-file-conversation.md`, plus
  `docs/dev/todo/2026-09-28-conversation-rs-crossed-the-file-budget.md`.
- `subagent.rs` — 1,985 production lines (was 1,912), +73 of planned seams; the predicate
  machinery lives in the new sibling `subagent/yield_condition.rs`. Open, unclaimed.
- `subagent_runtime.rs` — 729 (was 716), +13. Open, unclaimed.
- `tddy-session-agents` `service.rs` — 1,122 (was 1,076), +46. Open, unclaimed.

All restructuring is deferred past the `subagent-control` stack.

## Backlog

No prior `docs/dev/todo/` entry resolved: the seven-files-over-budget and frame-overflow entries
stay open as ⚠ DURING constraints; one entry added (the conversation.rs deferral above).
