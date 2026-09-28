# Changeset — resume with a replaced tool call (`resume-replacement`)

**Date:** 2026-09-27
**Status:** 🚧 In Progress
**Type:** Feature
**Stack:** `subagent-control` node 5 of 5 (`(#subagent-control 5/5)`), wave 3. **PR:** [#557](https://github.com/uppin/tddy-coder/pull/557) (draft).
**Branch:** `feature/subagent-control/resume-replacement` · **Base:** `feature/subagent-control/yield-conditions`

**PRD:** [`2026-09-27-resume-replacement-prd.md`](2026-09-27-resume-replacement-prd.md)
**Initial discovery:** [`2026-09-27-resume-replacement-initial-discovery.md`](2026-09-27-resume-replacement-initial-discovery.md)

## Prerequisites

- ✅ **Resolved here** —
  [the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md](../todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md):
  this PR's acceptance tests exercise `ResumeAgentConversation` **at the wire level** (real proto
  round-trip, not in-process calls), which is exactly the coverage that entry records as missing.
  At wrap, the entry's file is deleted by this node's `/wrap-context-docs`.
- ⚠ **During** —
  [seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md](../todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md)
  and the `oversized-file-{subagent,subagent-runtime,server,service}.md` code issues: the
  replacement types, validation and transcript-append go in **new modules**
  (`packages/tddy-discovery/src/subagent/replacement.rs`; a small transcript-append module beside
  `transcript.rs` if the append needs its own surface). The unavoidable in-place edits:
  `TurnRequest` field, the `take_turn` resume path (append after rewind/correction, ~10 lines),
  proto fields, MCP schema/arg parsing.
- ⚠ **During** —
  [a-conversation-id-is-not-bound-to-the-session-that-opened-it.md](../todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md)
  and the resume-RPC entry above: the replacement rides `ResumeAgentConversationRequest`, which
  the unbound-conversation entry also concerns; the wire-level tests here exercise the same
  surface without changing its binding semantics.

## Affected packages

- [`tddy-discovery`](../../../packages/tddy-discovery/README.md) — `TurnRequest` field, replacement types/validation/append (new modules), transcript append, RPC client `turn_call`
- [`tddy-session-agents`](../../../packages/tddy-session-agents/README.md) — server `within` replacement handling + framing
- [`tddy-service`](../../../packages/tddy-service/README.md) — `ResumeAgentConversationRequest` replacement fields
- [`tddy-tools`](../../../packages/tddy-tools/README.md) — MCP `subagent_resume` schema + parsing

## Responsibility

- Resume-with-replacement end to end: MCP args → validation → `TurnRequest` → wire RPC → server
  append (assistant tool-call message + caller-provided tool result, minted ids, rewind-then-append
  order) → the turn continuing over the appended history; wire-level tests of the resume RPC.

## Boundaries

- Append-only: the original failed call stays in history (approved design).
- Resume-only: no replacement on `subagent_prompt`.
- No dispatch of the replaced call — its result is the caller's text, recorded as history.
- No conditions/stop-reason changes (node 4); no summary changes (node 1).
- No UI changes.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n4` yield-conditions (#556) | `StopReason::YieldedToCaller` + the yielded tool message's id in the outcome; `yield_condition.rs` | the resume flow's trigger and the `fromMessageId` the caller learned from the yield; the same tool-name vocabulary for replacement validation | add stop reasons, touch `yield_condition.rs`, change the yield outcome's shape |
| `n1` tool-previews (#553) | `MessageDescriptor` + `resultSummary` plumbing | the appended tool message's descriptor is built by the existing plumbing | re-touch `MessageDescriptor` or add summary facts |
| `n2` grep-context (#554), `n3` agent-usage-notes (#555) | line-order predecessors | nothing | touch their surfaces |

## Draft PR contract

First push (wave 2, commit 2): `replacement.rs` — the `Replacement {tool, arguments, result}`
types, validation (known tool, `validate_tool_arguments` reuse, JSON-parseable bounded result),
the `Transcript::append_replacement` signature, `TurnRequest.replacement` + builder, the proto
fields — plus failing acceptance tests pinning the append order and the wire round-trip. No
dependent PRs exist above this node (top of the stack).

## Green wave

**Wave:** 3 of 3
**Greenable independently:** yes — its tests drive the transcript and the turn loop with node 4's
**published surface** (the yield machinery and its outcome shape, wave-2 commit 2 of #556), and
the wire-level RPC tests drive the proto with a scripted daemon. It cannot start before node 4's
wave-2 contract commit (its tests consume the yielded tool message's id and `StopReason`
variant).
**Concurrent with:** nothing — top of the stack, sole wave-3 member.
**Blocks:** nothing.

Real dependency edges, as opposed to the branch line:

    tool-previews → yield-conditions → resume-replacement

## State A

`ResumeAgentConversationRequest` carries `{from_message_id, correction, max_turns}`; the resume
path appends only a `user`-role text message for `correction` (subagent.rs:1745-1748);
`Transcript` has `push`/`push_marked` only — no append of a caller-supplied tool call + result;
`ChatMessage::assistant(tool_calls)` / `ChatMessage::tool_result(content, call_id, name)`
constructors exist but are used only by the turn loop.

## State B

A resume may carry a `replacement {tool, arguments, result}`: validated, appended after any
rewind and correction as an assistant tool-call message + tool result message with minted ids,
and the turn continues over the appended history. The appended call never dispatches; the result
is the caller's text. The whole thing travels the resume RPC, with wire-level tests.

## Delta

- `tddy-discovery`: new `subagent/replacement.rs` (types, validation); `Transcript` append
  (minted ids, marked as caller-provided); `TurnRequest.replacement` + builder; `take_turn`
  resume path threads append-after-rewind/correction; `turn_call` →
  `ResumeAgentConversationRequest` fields.
- `tddy-session-agents`: server-side replacement handling in the resume path + framing.
- `tddy-service`: `ResumeAgentConversationRequest` replacement fields.
- `tddy-tools`: `subagent_resume_schema` + parsing.

## Implementation milestones

- [ ] `replacement.rs` types + validation
- [ ] `Transcript` append of the caller-provided call + result
- [ ] `TurnRequest` + `take_turn` resume threading (rewind → correction → replacement order)
- [ ] RPC wire (client + server + proto)
- [ ] MCP schema + parsing
- [ ] Wire-level resume-RPC tests

## Testing plan

- `packages/tddy-discovery/tests/resume_replacement_acceptance.rs` — local sessions with a
  scripted model backend: append order, minted ids, continuation sees the replacement, rewind +
  replacement, validation rejections.
- Wire-level RPC tests (real proto round-trip over the roster link, scripted daemon side) —
  also closing the resume-RPC wire-test backlog entry.
- MCP tool tests in `tddy-tools` for the schema/parsing (existing
  `session_agent_conversation_client_acceptance` harness).

## Acceptance tests

1. `a_resume_with_a_replacement_appends_the_call_and_result_and_continues` —
   `packages/tddy-discovery/tests/resume_replacement_acceptance.rs`
2. `the_appended_messages_carry_minted_ids_and_appear_in_the_outcome` — same file
3. `a_rewind_then_replacement_resumes_from_the_replacement` — same file
4. `the_replaced_call_never_dispatches` — same file
5. `malformed_replacements_are_rejected_before_the_turn_runs` — unit tests in
   `packages/tddy-discovery/src/subagent/replacement.rs`
6. `resume_with_replacement_round_trips_the_rpc_wire` — wire-level RPC test (closes
   [the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md](../todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md))

## Technical debt & production readiness

(populated during development)

## Decisions & trade-offs

- **Keep original + append** (approved): the subagent sees both the failure and the operator's
  fix. Costs context bytes vs. rewrite-in-place, and the failure stays visible — chosen for
  honesty of history; a rewrite-in-place variant was rejected because it fabricates a history the
  subagent never had.
- The replacement result is JSON-**string** (caller's text, verbatim) rather than a structured
  proto — the transcript stores tool results as strings today, and the caller owns the shape.

## Refactoring needed

(none planned; new-module constraint above)

## Validation results

(to be filled by `/validate-changes`)

## TODO

- [x] Record initial discovery (`2026-09-27-resume-replacement-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — 4 unit (validation) + 3 acceptance (the append behaviours) + 1 wire-level RPC failures, each the missing transcript append; nodes 1–4's inherited red rides this branch and is theirs to green
- [x] USER REVIEW — acceptance tests — waived by the developer ("finish the remaining ones without stopping", 2026-09-27)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (scoped gates green:
  `./test -p tddy-discovery -p tddy-session-agents -p tddy-tools -p tddy-daemon-rpc`, 912 passed /
  0 failed across 122 binaries; scoped clippy + fmt clean). Green filled the three contract stubs.
  Three documented judgment calls: the replacement tool set is **derived from the advertised
  definitions** (ten tools — the summary vocabulary's nine misses `SEMANTIC_SEARCH`), not a
  hand-kept copy; the repeat-ledger invalidation now covers an appended replacement (outside the
  loop's changes, as its comment enumerates); replacement call ids mint as
  `call_replacement_{ordinal}` from the transcript's rising counter. Validation runs above the
  rewind, so a malformed replacement never reshapes the history it is refused from.- [ ] Update documentation with progress
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-27-resume-replacement-initial-discovery.md` and [the-resume-rpc wire-test entry](../todo/2026-09-26-the-resume-rpc-and-its-turn-budget-have-no-wire-level-test.md)
- [ ] USER REVIEW — work complete, decide next steps
