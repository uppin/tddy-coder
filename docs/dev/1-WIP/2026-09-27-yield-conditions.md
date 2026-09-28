# Changeset — turn yield conditions (`yield-conditions`)

**Date:** 2026-09-27
**Status:** 🚧 In Progress
**Type:** Feature
**Stack:** `subagent-control` node 4 of 5 (`(#subagent-control 4/5)`), wave 2. **PR:** [#556](https://github.com/uppin/tddy-coder/pull/556) (draft).
**Branch:** `feature/subagent-control/yield-conditions` · **Base:** `feature/subagent-control/agent-usage-notes`

**PRD:** [`2026-09-27-yield-conditions-prd.md`](2026-09-27-yield-conditions-prd.md)
**Initial discovery:** [`2026-09-27-yield-conditions-initial-discovery.md`](2026-09-27-yield-conditions-initial-discovery.md)
**Successor PRs:** `feature/subagent-control/resume-replacement` (consumes the yielded tool message's id)

## Prerequisites

- ⚠ **During** —
  [seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md](../todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md)
  and the `oversized-file-{subagent,subagent-runtime,server,service}.md` code issues: the
  predicate types, evaluation and validation go in **new modules**
  (`packages/tddy-discovery/src/subagent/yield_condition.rs` and a small framing module where
  needed). The unavoidable in-place edits: the `run_one_turn` tool-call loop check (one call +
  early return), the `TurnRequest` field, the proto fields, and one arm each at the five
  exhaustive `StopReason` sites.
- ⚠ **During** —
  [a-turns-message-list-can-overflow-the-chunk-framing-threshold.md](../todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md):
  the fired-condition report on the final frame is small and bounded (one condition echo); it is
  accounted against the same frame budget as node 1's `resultSummary` bytes.

## Affected packages

- [`tddy-discovery`](../../../packages/tddy-discovery/README.md) — `TurnRequest` field, predicate types/evaluation (new module), `StopReason` variant, RPC client `turn_call`/`parse_stop_reason`
- [`tddy-session-agents`](../../../packages/tddy-session-agents/README.md) — server `within`/`agent_turn_frames`/`agent_stop_reason` arms
- [`tddy-service`](../../../packages/tddy-service/README.md) — `session_agents.proto` fields on both request protos + fired-condition field on the chunk
- [`tddy-tools`](../../../packages/tddy-tools/README.md) — MCP schemas (`subagent_prompt`/`subagent_resume`), request construction

## Responsibility

- `yieldConditions` end to end: MCP args → `TurnRequest` → wire RPCs → server framing → predicate
  evaluation in the turn loop (after each tool result append) → `StopReason::YieldedToCaller`
  outcome with the fired condition and the yielded tool message's id → MCP JSON and proto
  spellings at every exhaustive site.

## Boundaries

- No conditions on model output; no conversation-scoped condition state.
- No resume-with-replacement (node 5).
- No new facts in `resultSummary` (node 1 owns the vocabulary; this node only reads it).
- No UI changes.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-previews (#553) | `result_summary.rs` per-tool fact extraction + fact names; `MessageDescriptor` widening | predicate `outcome` evaluation reads node 1's extracted facts (it will have node 1's surface in its tree after rebase) | add facts to `result_summary.rs`, re-touch `MessageDescriptor`, or change `prompt_outcome_json`'s existing fields |
| `n2` grep-context (#554) | Grep `before`/`after` | nothing — order only | touch `tool_grep`/`grep_limited` |
| `n3` agent-usage-notes (#555) | `usageNotes` | nothing — order only | touch def/registry/web surfaces |

## Draft PR contract

First push (wave 2, commit 2): `yield_condition.rs` — the `YieldCondition`/`When` types, the
`evaluate(condition, tool, args, summary)` signature `run_one_turn` will call, request
validation (unknown tool / unknown fact / bounds), `TurnRequest.yield_conditions`, the
`StopReason::YieldedToCaller` variant at every exhaustive site, and the proto fields — plus
failing acceptance tests pinning the yielded outcome's shape and each predicate kind.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** yes — its tests exercise the turn loop with node 1's **published
surface** (the extraction functions and fact names, which wave 2 publishes as its first push),
not node 1's behaviour beyond it; local sessions cover the loop directly, and RPC round-trip
tests drive the wire with a scripted backend. It cannot be greened before node 1's wave-2
contract commit (its predicate vocabulary compiles against node 1's types).
**Concurrent with:** nothing else in wave 2 (sole member).
**Blocks:** `resume-replacement` (node 5 — it needs the yielded tool message's id and the yield
machinery to resume from).

Real dependency edges, as opposed to the branch line:

    tool-previews → yield-conditions → resume-replacement

## State A

`TurnRequest` carries `{prompt, from_message, correction, max_turns}`; the turn loop feeds every
tool result back to the model unconditionally; `StopReason` has five variants; the RPCs carry
`max_turns` only as a turn-control field; the MCP schemas expose `maxTurns`/`fromMessageId`/
`correction`.

## State B

`TurnRequest.yield_conditions: Vec<YieldCondition>`; after each tool result append in
`run_one_turn`, the loop evaluates the request's conditions against the call's arguments and
node-1 summary; a match stops the turn with `StopReason::YieldedToCaller`, the fired condition,
and the tool message's id; validation rejects malformed conditions before the turn runs; the
whole thing travels on both request protos and back on the final chunk.

## Delta

- `tddy-discovery`: new `subagent/yield_condition.rs` (types, evaluation, validation);
  `TurnRequest` field + builder; `run_one_turn` check (early return, new outcome);
  `StopReason::YieldedToCaller` + `turn_stop_reason`/`parse_stop_reason` arms;
  `turn_call`/`PromptAgentConversationRequest`/`ResumeAgentConversationRequest` fields;
  `prompt_outcome_json` fired-condition field.
- `tddy-session-agents`: `agent_stop_reason` arm; `within` → request conditions; chunk framing of
  the fired condition.
- `tddy-service`: proto fields (`repeated YieldCondition yield_conditions` on both requests — or
  JSON-string per the `stop_reason` precedent, decided in wave 2 — plus `fired_condition` on the
  final chunk).
- `tddy-tools`: MCP schemas + arg parsing on both tools.

## Implementation milestones

- [x] `yield_condition.rs` types + evaluation + validation
- [x] `TurnRequest` field + loop check + new stop reason at every site
- [x] RPC wire both directions (proto + client `turn_call`)
- [x] MCP schemas + parsing
- [x] Fired-condition report bounded for the frame budget

## Testing plan

- `packages/tddy-discovery/tests/yield_conditions_acceptance.rs` — local-session turns with a
  scripted model backend: outcome predicate yields, argument predicate yields, non-matching
  conditions are invisible, transcript ends at the tool result, yielded id is the tool message's.
- `packages/tddy-discovery/src/subagent/yield_condition.rs` (unit) — evaluation against each fact
  kind, validation rejections (unknown tool, unknown fact for the tool, bounds, count).
- RPC round-trip: `tddy-daemon-rpc`/`tddy-discovery` roster tests with a scripted daemon.
- Stop-reason spelling tests extended at the existing four spelling-test sites.

## Acceptance tests

1. `an_outcome_condition_yields_the_turn_at_the_matching_tool_call` —
   `packages/tddy-discovery/tests/yield_conditions_acceptance.rs`: STR_REPLACE matching nothing
   under `{matchedLines: 0}` yields with the fired condition named and the tool message's id.
2. `an_argument_condition_yields_on_a_contained_substring` — same file: WRITE with `TODO(` in
   `contents` under `{argument: {field: "contents", contains: "TODO("}}` yields after the call.
3. `a_non_matching_condition_is_invisible_in_the_outcome` — same file.
4. `a_yielded_turn_ends_its_transcript_at_the_tool_result` — same file: no further model turn.
5. `malformed_conditions_are_rejected_before_the_turn_runs` — unit tests in
   `packages/tddy-discovery/src/subagent/yield_condition.rs`.
6. `yield_conditions_round_trip_the_conversation_rpc` — RPC test, scripted backend.

## Technical debt & production readiness

(populated during development)

## Decisions & trade-offs

- `argument` predicates evaluate **after dispatch**, on the call as made — the call's result
  stays in the transcript (the work happened; hiding it would fabricate history). A pre-dispatch
  argument gate is a possible future refinement, deliberately not built here.
- Conditions are per-turn-request (stateless) — conversation-scoped standing rules would need
  state, eviction and visibility; not needed for the use cases named.
- `outcome` predicates are equality-only (`{matchedLines: 0}`); no comparators — equality on the
  named facts covers the cases named, and a comparator DSL is scope creep until a case demands it.

## Refactoring needed

(none planned; new-module constraint above)

## Validation results

(to be filled by `/validate-changes`)

## TODO

- [x] Record initial discovery (`2026-09-27-yield-conditions-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — 4 unit (evaluate/validate) + 3 acceptance (the yield behaviours) failures, each the missing loop evaluation; nodes 1–3's inherited red rides this branch and is theirs to green
- [x] USER REVIEW — acceptance tests — waived by the developer ("finish the remaining ones without stopping", 2026-09-27)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (scoped gates green: `./test -p tddy-discovery
  -p tddy-session-agents -p tddy-tools`, 88 suites / 0 failed; scoped clippy clean). Green filled
  the two contract stubs (`evaluate`, `validate`) and wired their live call sites: the turn-entry
  validation and the post-append loop check. One contract-commit defect fixed beyond the listed
  seams: `tddy-session-agents`' request parser silently dropped an unparseable
  `yieldConditions` payload — its own doc comment promised an error naming the request; it now
  returns one (as `Status::invalid_argument`, before any turn is stamped). A pre-existing clippy
  `-D warnings` failure in `tddy-tools` (dead `mut`) also blocked the gate and was fixed with a
  one-token change.- [ ] Update documentation with progress
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-27-yield-conditions-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
