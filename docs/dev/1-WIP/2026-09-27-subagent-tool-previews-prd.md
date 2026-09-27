# PRD — subagent tool result summaries (`tool-previews`, node 1 of `subagent-control`)

**Stack:** `subagent-control` — this is node 1 of 5 (wave 1). Base: `master`.
**Branch:** `feature/subagent-control/tool-previews`

## Problem

A turn outcome's tool-role `MessageDescriptor` carries `preview` — the first 240 chars of the raw
tool-result text — and `isError`. That is all a main agent learns about what a subagent's tool
calls did. For a `READ` the preview is the beginning of the file contents (indistinguishable from
any other text); for a `STR_REPLACE` there is no way to see how much was replaced. The numbers a
caller needs to reason with — chars read, lines matched — are buried in the result JSON and
truncated away.

## What this PR delivers

A structured **`resultSummary`** on tool-role `MessageDescriptor`s in the turn outcome, built from
the tool's own result JSON at the single point where results are appended
(`run_one_turn`, subagent.rs:1408):

```json
{ "role": "tool", "tool": "READ", "isError": false,
  "preview": "use crate::foo…",                       // unchanged raw text
  "resultSummary": { "read": { "firstLine": "use crate::foo;", "charsRead": 42013 } } }
```

Per-tool summaries (new module in `tddy-discovery`, not growth of `subagent.rs`):

| Tool | `resultSummary` facts |
|---|---|
| `READ` | `firstLine` (first non-empty line, bounded), `charsRead`, `totalLines`, `truncated` |
| `GREP` | `matchCount`, `truncated`, `totalMatches` |
| `GLOB` | `pathCount`, `truncated`, `totalPaths` |
| `STR_REPLACE` | `replaced`, `matchedLines` (see below), `bytesWritten` |
| `WRITE` | `bytesWritten` |
| `DELETE` | `deleted` |
| `SHELL` | `exitCode`, `stdoutChars` (blocking); `jobId` (background) |
| `AWAIT` | `exitCode`, `completed` |
| `READ_LINTS` | `lintCount` |
| error/rejected/repeated dispatches | `{ "error": true }` — summary of the failure shape, not the text |

`matchedLines` on `STR_REPLACE` **requires the tool engine to return its internal occurrence
count** (`tool_str_replace`, `packages/tddy-tool-engine/src/lib.rs:384-390` counts occurrences to
enforce uniqueness today and discards the count). This PR adds `matchedOccurrences` to the engine
result JSON and derives `matchedLines` from it.

## Acceptance criteria

1. Every tool-role `MessageDescriptor` in a `PromptOutcome` carries a `resultSummary` object with
   the per-tool facts above, on the local-session path.
2. The summary is serialized in `prompt_outcome_json` (MCP) and on the proto final frame
   (`AgentMessageDescriptor` + `message_descriptor` + `parse_message_descriptor`), both directions
   of the RPC.
3. `STR_REPLACE` engine results expose the occurrence count; the summary reports `matchedLines`.
4. `resultSummary` on an error/rejected/repeated dispatch reports the failure shape, and
   `isError` remains the authoritative flag.
5. Summaries are bounded — no unbounded strings (a bounded `firstLine` is the only text); the
   added bytes per descriptor are counted against the final LiveKit frame budget
   (`a-turns-message-list-can-overflow-the-chunk-framing-threshold`).
6. `tool_calls` on the wire stays names-only in this PR (its widening is the standing TODO at
   conversation.rs:422, not this node's).

## Boundaries

- No yield conditions, no stop-reason change, no resume change — nodes 4/5.
- No new tools, no changes to what tools return beyond the STR_REPLACE count (node 2 owns Grep's
  shape).
- `usageNotes` is node 3.
- The plain `preview` field is unchanged.

## Dependencies

None — root node. `master` is the base.

## Successor PRs

- `feature/subagent-control/yield-conditions` — evaluates the facts this PR extracts as yield
  predicates.
