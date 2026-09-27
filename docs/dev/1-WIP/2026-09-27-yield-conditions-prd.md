# PRD — turn yield conditions (`yield-conditions`, node 4 of `subagent-control`)

**Stack:** `subagent-control` — node 4 of 5 (wave 2). Base: `feature/subagent-control/agent-usage-notes`.
**Branch:** `feature/subagent-control/yield-conditions`

## Problem

The only mid-turn control a caller has is `maxTurns`. If a subagent's `STR_REPLACE` matches
nothing and it keeps hammering the same edit, or it writes a file containing `TODO(`, the main
agent learns nothing until the turn budget expires — and then only as a pile of message
descriptors. The caller could see the problem **at the tool call** if it could say so up front:
"stop the turn and hand control back to me the moment this happens."

## What this PR delivers

**`yieldConditions`** on a turn request (MCP `subagent_prompt` / `subagent_resume`, and the
`PromptAgentConversation`/`ResumeAgentConversation` RPCs) — a list of structured predicates
evaluated after each tool call's result is appended:

```json
"yieldConditions": [
  { "tool": "STR_REPLACE", "when": { "outcome": { "matchedLines": 0 } } },
  { "tool": "WRITE", "when": { "argument": { "field": "contents", "contains": "TODO(" } } }
]
```

Two predicate kinds, both bounded:

- **`outcome`** — a fact from the call's `resultSummary` (node 1's extraction) compared to an
  equality: `{matchedLines: 0}`, `{exitCode: 1}`, `{matchCount: 0}`, `{error: true}`.
- **`argument`** — a string field of the call's arguments contains a substring:
  `{field: "contents", contains: "TODO("}`.

Behaviour when a condition matches:

1. The turn stops **immediately** — the tool result already appended stays in the transcript; the
   model is **not** sent the result and does not take another step.
2. The outcome returns `stopReason: "yieldedToCaller"`, and identifies **which condition fired**
   (`whichCondition: {"tool": "STR_REPLACE", "when": …}`) and **the tool message's id** — the id
   `subagent_resume` will consume in node 5.
3. `usage` and `messages` cover everything the turn did up to and including the yielded call.
4. Conditions are per-turn-request (stateless): they apply for this turn only, not the
   conversation.

Validation: `tool` must be a known tool name; `outcome` fact names must be facts that tool's
`resultSummary` can carry; `contains` bounded (e.g. ≤ 256 chars); at most a handful of conditions
per request (e.g. ≤ 8). Unknown shapes are rejected before the turn runs.

## Acceptance criteria

1. A prompt with a yield condition whose predicate matches a tool call's outcome stops the turn
   at that call: `stopReason == "yieldedToCaller"`, the fired condition is named, and the
   transcript ends with the tool result (not a further model turn).
2. An `argument` predicate on an unexecuted-yet field (e.g. WRITE's `contents`) yields **before**
   the call dispatches? — no: `argument` is evaluated **after dispatch, on the call as made**;
   the result of the call stays in the transcript (see Decisions).
3. A non-matching condition leaves the turn indistinguishable from one run without conditions.
4. Conditions travel on the RPC wire (both request protos) and round-trip for remote
   conversations.
5. Malformed conditions (unknown tool, unknown fact for that tool, oversized `contains`, too
   many) are rejected before the turn starts, with the offending condition named.
6. `StopReason::YieldedToCaller` is spelled consistently at all five exhaustive sites + the proto
   string spelling; existing stop-reason spellings are untouched.

## Boundaries

- No condition on *model output* (only tool-call outcomes and arguments).
- No evaluation of conditions against anything but this turn's calls.
- No resume-with-replacement (node 5).
- No new tools, no resultSummary changes (node 1 owns the facts).
- No UI changes.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-previews | `resultSummary` extraction (`result_summary.rs`) + per-tool fact names; `MessageDescriptor` widening | predicate evaluation reads the same facts extracted for the descriptor; `outcome` fact names are node 1's fact vocabulary | add new facts to `result_summary.rs`, change `MessageDescriptor` again, or touch `prompt_outcome_json`'s existing fields |
| `n2` grep-context, `n3` agent-usage-notes | line-order predecessors, no consumed surface | nothing | touch their surfaces |

## Successor PRs

- `feature/subagent-control/resume-replacement` — resumes from the yielded tool message's id.
