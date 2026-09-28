# 2026-09-28 — A caller's condition on a tool call yields a subagent's turn back

PRD: the `yield-conditions` node of the `subagent-control` stack
([#556](https://github.com/uppin/tddy-coder/pull/556)).

`subagent_prompt` and `subagent_resume` accept **`yieldConditions`** — up to 8 conditions, each
naming a tool and what to watch for on its call: an **`outcome`** fact compared for equality
against the tool's result summary (`{matchedLines: 0}`, `{exitCode: 1}`, `{error: true}`, …), or
an **`argument`** string field containing a substring (≤256 chars). Conditions are per-turn —
they apply to this request only.

When a condition matches, the turn stops **at that call**: the tool result stays in the
transcript, the model is never sent it and takes no further step, and the outcome returns
`stopReason: "yieldedToCaller"` naming the fired condition and the tool message's id — the anchor
a later resume can consume. A condition's facts are the same ones the turn outcome's
`resultSummary` reports, so what a caller can watch is exactly what it can see.

Malformed conditions are refused before the turn runs — an unknown tool, a fact that tool's
summary cannot carry (a `matchedLines` condition on a `WRITE` is a refusal, never a condition
that silently never fires), an over-long needle, or too many — each naming the offending
condition, so a malformed request costs no model turn.
