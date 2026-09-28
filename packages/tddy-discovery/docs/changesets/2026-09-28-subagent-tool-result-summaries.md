# 2026-09-28 — A subagent turn outcome reports each tool call's result as structured facts

**Type:** Feature

`#subagent-control` 1/5, [#553](https://github.com/uppin/tddy-coder/pull/553). Cross-package entry:
`docs/dev/changesets/2026-09-28-subagent-tool-result-summaries.md`.

A tool-role `MessageDescriptor` carries a **`resultSummary`**: the structured facts of that tool's
result, extracted from the result JSON at the transcript's append site (`run_one_turn`) and
serialized as one externally tagged object — `{"read": {…}}`, `{"strReplace": {…}}` — that a caller
dispatches on like the tool call itself. `READ` reports `{firstLine, charsRead, totalLines,
truncated}` (the first non-empty line cut to 120 chars is the only text; `charsRead` counts
characters excluding newlines); `GREP`/`GLOB` report match/path counts and totals; `STR_REPLACE`
reports `{replaced, matchedLines, bytesWritten}` from the engine's `matchedOccurrences`; `SHELL`
reports the ending or the background job's id; `AWAIT`, `WRITE`, `DELETE`, `READ_LINTS` report
theirs. A dispatch that produced nothing summarizes as `{"error": true}`; `isError` stays
authoritative. Absent collection fields default to empty facts, so a summary is per-tool, not
per-result-shape, and every summary is bounded (~600 bytes worst case per tool-role descriptor on
the final LiveKit frame).

The extraction lives in the new `subagent/result_summary.rs` (`summarize`, the `ResultSummary`
vocabulary, 11 in-file tests); `subagent.rs` keeps only the wiring (`ToolDispatch::summary`, the
`push_tool_result` append site) and `subagent/transcript.rs` the descriptor field. The proto final
frame carries it as `result_summary_json`, parsed back in `roster/conversation.rs`.

Code issue at wrap: `docs/code-issues/oversized-file-subagent.md` — 1,850 production lines (was
1,832), +18 wiring, extraction in the new sibling. Open, unclaimed; restructuring deferred past
the `subagent-control` stack, whose sibling nodes touch the file.
