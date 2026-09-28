# Changeset — subagent tool result summaries (`tool-previews`)

**Date:** 2026-09-27
**Status:** 🚧 In Progress
**Type:** Feature
**Stack:** `subagent-control` node 1 of 5 (`(#subagent-control 1/5)`), wave 1. **PR:** [#553](https://github.com/uppin/tddy-coder/pull/553) (draft).
**Branch:** `feature/subagent-control/tool-previews` · **Base:** `master`

**PRD:** [`2026-09-27-subagent-tool-previews-prd.md`](2026-09-27-subagent-tool-previews-prd.md)
**Initial discovery:** [`2026-09-27-subagent-tool-previews-initial-discovery.md`](2026-09-27-subagent-tool-previews-initial-discovery.md)
**Successor PRs:** `feature/subagent-control/yield-conditions` (consumes the facts extracted here)

## Prerequisites

- ⚠ **During** —
  [a-turns-message-list-can-overflow-the-chunk-framing-threshold.md](../todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md):
  `resultSummary` adds bytes to every tool-role descriptor on the final LiveKit frame. This PR
  keeps every summary bounded (no unbounded strings; the only text is a `firstLine`, itself
  bounded) and records the added per-descriptor byte cost in its frame-budget accounting.
- ⚠ **During** —
  [seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md](../todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md):
  the summary extraction, serialization and proto plumbing go in **new modules**
  (`packages/tddy-discovery/src/subagent/result_summary.rs` and, where needed, new small files in
  `tddy-session-agents`/`tddy-tool-engine`), not as growth of `subagent.rs`, `service.rs`,
  `server.rs` or tool-engine `lib.rs`. The unavoidable in-place edits at `run_one_turn`'s append
  site and the STR_REPLACE result JSON stay minimal.
- ⚠ **During** —
  [`oversized-file-subagent.md`](../../../packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md),
  [`oversized-file-subagent-runtime.md`](../../../packages/tddy-discovery/docs/code-issues/oversized-file-subagent-runtime.md),
  [`oversized-file-service.md`](../../../packages/tddy-session-agents/docs/code-issues/oversized-file-service.md),
  [`oversized-file-lib.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md):
  same constraint as above — new modules for new code; no net growth beyond the minimal seams.
  All are open and unclaimed; none is resolved here.

## Affected packages

- [`tddy-discovery`](../../../packages/tddy-discovery/README.md) — summary extraction, `MessageDescriptor`, RPC framing
- [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md) — `tool_str_replace` exposes the occurrence count
- [`tddy-session-agents`](../../../packages/tddy-session-agents/README.md) — proto `AgentMessageDescriptor` + server framing
- [`tddy-service`](../../../packages/tddy-service/README.md) — proto file (`session_agents.proto`)

## Responsibility

- `resultSummary` on every tool-role `MessageDescriptor` in a turn outcome — per-tool structured
  facts (`read.firstLine/charsRead/totalLines/truncated`, `grep.matchCount/totalMatches`,
  `strReplace.replaced/matchedLines/bytesWritten`, `shell.exitCode/stdoutChars`, `write.bytesWritten`,
  `delete.deleted`, `await.exitCode/completed`, `readLints.lintCount`, error dispatches as
  `{error: true}`).
- `matchedOccurrences` in the STR_REPLACE engine result JSON (the count it already computes
  internally to enforce uniqueness).
- Serialization of the summary on the MCP turn outcome (`prompt_outcome_json`) and the proto
  final frame, both directions (`message_descriptor`, `parse_message_descriptor`).

## Boundaries

- No yield conditions, no new `StopReason` variant, no resume changes (nodes 4/5).
- No Grep shape changes (`before`/`after` is node 2).
- No `usageNotes` (node 3).
- The wire's `tool_calls` stays names-only — its widening is the standing TODO at
  `roster/conversation.rs:422`, not this node's.
- The plain `preview` field and `MESSAGE_PREVIEW_CHARS` are unchanged.

## Dependencies

None — root node. Everything this PR consumes exists on `master`.

## Draft PR contract

First push (wave 2, commit 2): `result_summary.rs` — the `ResultSummary` types, the per-tool
extraction functions with the exact signatures `run_one_turn` will call, `MessageDescriptor`
with its new `result_summary` field, the STR_REPLACE `matchedOccurrences` engine field — plus
failing acceptance tests pinning the summary shapes per tool and the RPC round-trip. Dependents
(`yield-conditions`) branch off those types.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — tests exercise the extraction module, the descriptor
serialization and the engine result; no other node's behaviour is needed.
**Concurrent with:** the `grep-context` and `agent-usage-notes` nodes.
**Blocks:** `yield-conditions` (its predicates evaluate the facts this PR extracts).

Real dependency edges, as opposed to the branch line:

    tool-previews → yield-conditions → resume-replacement

## State A

A tool-role `MessageDescriptor` carries `{id, role, tool, tool_calls, is_error, preview}` — the
raw result text cut to 240 chars and a boolean. The result JSON is stringified into the
transcript (`tool_result_payload`, subagent.rs:821-830) and its structure is lost to the caller.
`tool_str_replace` computes the occurrence count internally (lib.rs:384-390) and discards it.

## State B

Every tool-role descriptor carries a bounded, structured `resultSummary` built at the transcript
append site from the `ToolDispatch::Ran(value)` JSON before it is stringified. The summary
travels: MCP turn outcome (`prompt_outcome_json`), proto final frame (`AgentMessageDescriptor`,
`message_descriptor` server-side, `parse_message_descriptor` client-side). STR_REPLACE's engine
result reports `matchedOccurrences`; the summary derives `matchedLines`.

## Delta

- `tddy-discovery`: new `subagent/result_summary.rs` (types + per-tool extraction); one call at
  the `push_marked` site in `run_one_turn`; `MessageDescriptor.result_summary` in `transcript.rs`;
  proto plumbing in `roster/conversation.rs` (`parse_message_descriptor`).
- `tddy-tool-engine`: `tool_str_replace` result gains `matchedOccurrences` (in-place, ~3 lines).
- `tddy-session-agents`: `message_descriptor` maps the new field (new small module if the
  function moves; otherwise minimal in-place).
- `tddy-service`: `session_agents.proto` `AgentMessageDescriptor` gains `optional string
  result_summary_json = 7` (summary serialized as JSON string — string-typed, like `stop_reason`).

## Implementation milestones

- [x] Result summary types + per-tool extraction (`subagent/result_summary.rs`)
- [x] STR_REPLACE `matchedOccurrences` in the engine result
- [x] `MessageDescriptor.result_summary` + `push_marked` plumbing
- [x] MCP + proto serialization, both RPC directions
- [x] Frame-budget accounting note (bounded bytes per descriptor)

## Testing plan

Unit + integration (no E2E needed; the MCP/RPC surfaces have existing test harnesses):

- `packages/tddy-discovery/tests/tool_result_summary_acceptance.rs` — a turn whose tool calls
  ran READ/GREP/STR_REPLACE/SHELL reports the per-tool summaries on the outcome's descriptors.
- `packages/tddy-discovery/src/subagent/result_summary.rs` (unit) — each tool's fact extraction
  from its result JSON, including error/rejected/repeated dispatch shapes.
- `packages/tddy-tool-engine/tests/str_replace_occurrence_count.rs` — `matchedOccurrences` in
  the result, unique and non-unique cases (non-unique errors as today).
- `packages/tddy-session-agents` (or `tddy-discovery` roster) — proto round-trip of
  `resultSummary` on the final frame.

## Acceptance tests

1. `a_read_tool_message_carries_a_structured_result_summary` —
   `packages/tddy-discovery/tests/tool_result_summary_acceptance.rs`: a READ result's descriptor
   reports `{read: {firstLine, charsRead, totalLines, truncated}}` in the outcome.
2. `a_str_replace_summary_reports_matched_lines` — same file: after a successful STR_REPLACE the
   descriptor reports `{strReplace: {replaced: true, matchedLines: 1, bytesWritten}}`.
3. `a_failed_dispatch_summary_reports_the_error_shape` — same file: a rejected tool call's
   descriptor reports `{error: true}` and `isError: true`.
4. `result_summary_survives_the_proto_round_trip` — proto framing test: the descriptor's
   `resultSummary` parses back identically client-side.
5. `str_replace_returns_its_occurrence_count` —
   `packages/tddy-tool-engine/tests/str_replace_occurrence_count.rs`.

## Technical debt & production readiness

(populated during development)

## Decisions & trade-offs

- Summary serialized as a JSON **string** on the proto (`result_summary_json`), matching
  `stop_reason`'s string-typed precedent, instead of a nested proto message — avoids generated
  per-tool oneofs and keeps the wire additive.
- `matchedLines` derived from `matchedOccurrences` at the summary layer, not a separate engine
  count — one number, one source.

## Refactoring needed

(none planned; new-module constraint above)

## Validation results

(to be filled by `/validate-changes`)

## TODO

- [x] Record initial discovery (`2026-09-27-subagent-tool-previews-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — 11 unit + 3 acceptance + 2 engine failures, each attributable to the missing extraction/engine count
- [x] USER REVIEW — acceptance tests — waived by the developer ("finish the remaining ones without stopping", 2026-09-27)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (scoped gates green: `./test -p tddy-discovery
  -p tddy-tool-engine -p tddy-session-agents`, 0 failed; scoped clippy clean)
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-27-subagent-tool-previews-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
