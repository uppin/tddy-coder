# 2026-09-28 — Subagent turn outcomes report each tool call's result as structured facts

PRD: the `tool-previews` node of the `subagent-control` stack
([#553](https://github.com/uppin/tddy-coder/pull/553)).

A tool-role message in a subagent turn outcome now carries a **`resultSummary`** next to its
`preview`: the tool's own numbers, extracted from the result JSON instead of buried in text a
240-character preview truncates away. A `READ` reports `{firstLine, charsRead, totalLines,
truncated}`; `GREP` and `GLOB` report match/path counts and totals; `STR_REPLACE` reports
`{replaced, matchedLines, bytesWritten}`; `SHELL` reports the exit code and output size, or the
background job's id; `AWAIT`, `WRITE`, `DELETE` and `READ_LINTS` report their endings. The summary
serializes as one externally tagged object — `{"read": {…}}` — so the main agent dispatches on the
same name it dispatched the tool call on, and it rides the turn outcome on MCP and on the proto
final frame alike. A dispatch that produced no result summarizes as `{"error": true}`, with
`isError` remaining the authoritative flag.

Summaries are bounded — the only text is a 120-char `firstLine` — so the added cost on the final
LiveKit frame is capped at ~600 bytes per tool-role message. The underlying frame-overflow risk
stays tracked in `docs/dev/todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md`.

Supporting engine change: `StrReplace` reports `matchedOccurrences` in its result JSON, so a
caller sees how much an edit matched instead of inferring it from `replaced` alone.
