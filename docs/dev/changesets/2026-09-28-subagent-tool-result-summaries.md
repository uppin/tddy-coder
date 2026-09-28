# 2026-09-28 — A subagent turn outcome reports each tool call's result as structured facts

**Type:** Feature

`#subagent-control` 1/5, [#553](https://github.com/uppin/tddy-coder/pull/553) (base `master`).
The successors are `feature/subagent-control/{yield-conditions,resume-replacement,grep-context,agent-usage-notes}`.

Packages: `tddy-discovery` (the extraction and the descriptor field), `tddy-tool-engine`
(the STR_REPLACE occurrence count), `tddy-session-agents` (the proto final-frame mapping),
`tddy-service` (the proto field).

## What was delivered

Every tool-role `MessageDescriptor` in a subagent turn outcome carries a **`resultSummary`** —
the structured facts of that tool's result, extracted from the result JSON at the one point the
transcript appends it (`run_one_turn`), where the structure is still visible before the payload is
stringified into the tool message. The summary is serialized as one externally tagged object —
`{"read": {…}}`, `{"strReplace": {…}}` — so a caller dispatches on the same name it dispatches the
tool call on, and it travels the MCP turn outcome (`prompt_outcome_json`) and the proto final
frame (`AgentMessageDescriptor.result_summary_json`, string-typed like `stop_reason`) in both RPC
directions (`message_descriptor`, `parse_message_descriptor`).

Per tool: `READ` reports `{firstLine, charsRead, totalLines, truncated}` (`firstLine` is the first
non-empty line, cut to 120 chars — the only text a summary carries; `charsRead` counts characters
excluding the newlines `totalLines` already accounts for); `GREP`/`GLOB` report match/path counts
and totals; `STR_REPLACE` reports `{replaced, matchedLines, bytesWritten}`; `SHELL` reports the
exit code and output size, or the background job's id; `AWAIT` reports completion and exit code;
`WRITE`, `DELETE` and `READ_LINTS` report their ending. A dispatch that produced no result — a
failure, a rejection, a repeat — summarizes as `{"error": true}`, with `isError` remaining the
authoritative flag. Absent collection fields default to empty facts rather than an error, so a
summary is per-tool, not per-result-shape.

Every summary is bounded: no unbounded strings, so a descriptor's added byte cost on the final
LiveKit frame is capped at ~600 bytes per tool-role message on top of the ~960-byte preview bound
(recorded in the accounting of
`docs/dev/todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`, whose
defect itself remains open).

`tool_str_replace` now reports its internal occurrence count as `matchedOccurrences` in the engine
result JSON — on the success path and on the no-match error path (`0`, still an error); the
non-unique error is unchanged. The summary derives `matchedLines` from that count.

## Code-issue measurements at wrap

- `packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md` — 1,850 production lines
  (was 1,832 at the merge-base), +18 wiring; the extraction went to the new sibling
  `subagent/result_summary.rs`. Open, unclaimed.
- `packages/tddy-session-agents/docs/code-issues/oversized-file-service.md` — 1,076 (+7, one
  field mapping). Open, unclaimed.
- `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` — 777 (+14, both result
  paths plus the shared `ToolOutcome::err_json` constructor). Open, unclaimed.

Restructuring these is deferred past the `subagent-control` stack — its sibling nodes touch all
three files.

## Backlog

No `docs/dev/todo/` entry was resolved by this change; the three entries its planning scanned
(frame overflow, file budgets, oversized files) are all ⚠ DURING constraints this change honoured
and stay open.
