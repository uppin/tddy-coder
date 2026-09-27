# PRD — resume with a replaced tool call (`resume-replacement`, node 5 of `subagent-control`)

**Stack:** `subagent-control` — node 5 of 5 (wave 3). Base: `feature/subagent-control/yield-conditions`.
**Branch:** `feature/subagent-control/resume-replacement`

## Problem

When node 4's yield condition fires — `STR_REPLACE` matched nothing, say — the conversation is
parked at the failed call. The main agent's only resume options are `subagent_resume`
`{fromMessageId}` (rewind, discarding the failed attempt) or `{correction}` (a user-role text
message). Neither expresses what an operator most often wants: *"here is the call I meant to make
and the result it would have produced — continue from that."* Rewinding hides the failure from
the subagent; a text correction makes it re-derive the call. Neither lets the caller hand the
subagent a **result**.

## What this PR delivers

**Resume with a replacement** — the chosen design (keep original + append): the yielded
conversation keeps its history untouched and appends, after the original failed call:

1. an **`assistant` message carrying the caller's replacement tool call** (tool name +
   arguments, as the caller specifies), and
2. a **`tool` message carrying the caller's provided result** for that call.

Then the turn continues from there — the subagent sees both its own failed attempt and the
operator's fix, in the same shape a real call would have appeared in.

MCP surface (`subagent_resume` gains, alongside `fromMessageId`/`correction`/`maxTurns`):

```json
{ "sessionId": "…",
  "fromMessageId": "m42",                      // optional: rewind point, as today
  "replacement": {
    "tool": "STR_REPLACE",
    "arguments": {"path": "src/lib.rs", "old_string": "…", "new_string": "…"},
    "result": "{\"replaced\": true, \"bytes_written\": 128, \"matchedOccurrences\": 1}"
  } }
```

- `replacement.tool` must be a known tool name; `replacement.arguments` must pass the same
  `validate_tool_arguments` the real dispatch path applies; `replacement.result` must be
  JSON-parseable and bounded (e.g. ≤ 16 KiB) — all rejected before the turn runs, naming the
  offending field.
- The replacement appends **after** any `fromMessageId` rewind — rewind then append, in that
  order, so both can be given.
- `correction` and `replacement` may both be given (correction first, then the replacement).
- The appended messages get minted transcript ids like any other; they appear in the outcome's
  `messages` from the resume's `appended_from`.
- The turn continues with the caller's remaining budget (`maxTurns` as usual).
- **No tool actually dispatches**: the replacement result is the caller's text, recorded as
  history. (The appended call is marked so it is never re-counted as work.)

RPC surface: `ResumeAgentConversationRequest` gains the replacement fields; the remote path
appends identically (the append happens wherever the conversation lives — server-side framing in
`within`, client-side construction in `turn_call`).

## Acceptance criteria

1. A resume with a replacement on a yielded conversation appends the assistant tool-call message
   and the tool result message in that order, and the turn continues — the model's next request
   includes them.
2. The appended messages carry minted ids and appear in the outcome's `messages`.
3. Rewind + replacement works in that order (rewind to the failed call's assistant message, then
   append the replacement).
4. The replacement never dispatches; its result is the caller's text, verbatim in the
   transcript.
5. Malformed replacements (unknown tool, invalid arguments, unparseable/oversized result) are
   rejected before the turn runs, naming the field.
6. The replacement travels the `ResumeAgentConversation` RPC and appends identically on the
   remote path — with **wire-level tests** (closing
   `2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md`).

## Boundaries

- No transcript rewriting/deletion — history is append-only; the original failed call stays
  (approved design decision).
- No replacement on `subagent_prompt` (resume-only; a fresh prompt has nothing to replace).
- No conditions, no stop-reason changes (node 4), no summary changes (node 1).
- No UI changes.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n4` yield-conditions (#556) | `StopReason::YieldedToCaller`, the yielded tool message's id in the outcome, `yield_condition.rs` types | the resume flow's natural trigger and the `fromMessageId` the caller learned from the yield; the replacement validates against the same tool vocabulary | add stop reasons, touch `yield_condition.rs`, change the yield outcome's shape |
| `n1` tool-previews (#553) | `MessageDescriptor`/`resultSummary` | the appended tool message's descriptor is built the same way every tool message is (node 1's plumbing) | re-touch `MessageDescriptor` or add facts |
| `n2` grep-context (#554), `n3` agent-usage-notes (#555) | line-order predecessors | nothing | touch their surfaces |
