# oversized-file: subagent.rs

**Location:** `packages/tddy-discovery/src/subagent.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — hand-measured during `/plan-red` Step 2b for the
subagent turn-control changeset
**Metrics:** **1,208 production lines** · budget 500 · **2.4× over** · **no `#[cfg(test)]` module
at all**
**Thresholds breached:** length 1,208 > 500
**Restructure:** required — `extract_module --to_file` × 2, `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | In-file tests | Note |
|---|---|---|---|
| 2026-09-26 | 1,208 | none | first detection |
| 2026-09-26 | 1,324 | none | after the all-tools-failed error path (`ToolDispatch`, `ToolCallTally`) |
| 2026-09-26 | 1,431 | none | after per-call turn budgets, message ids and resume/rewind. Two new sibling modules absorbed what could be moved — `subagent/transcript.rs` (242 production lines, 6 in-file tests) and `subagent/turn_request.rs` (123, 4) — so the growth here is only what has to stay: `PromptOutcome`'s two new fields, `SubagentSession::take_turn`, and the `take_turn`/`run_turn_loop` split |

## What the tool found

Hand measurement: 1,208 lines, and `grep -n '#\[cfg(test)\]'` returns **nothing**. Every test that
covers this file is an integration test in `packages/tddy-discovery/tests/`
(`subagent_session_red.rs`, `subagent_loop_red.rs`, `subagent_context_exhaustion_red.rs`,
`subagent_usage_red.rs`, `specialized_subagent_red.rs`, `read_window_red.rs`,
`read_output_cap_red.rs`, `subagent_write_tools_red.rs`), so nothing exercises a private function
directly.

**Not measured:** complexity, nesting, CRAP. The CRAP pipeline was not run for this package —
`packages/tddy-discovery/docs/code-issues/` did not exist before this record, so the package had
never been analyzed at all.

## Why it matters here

The file carries at least four separable concerns:

| Concern | Roughly |
|---|---|
| Wire/vocabulary types — `ContentBlock`, `StopReason`, `PromptOutcome`, `SubagentError`, the `SubagentSession` trait | `:25-121` |
| `CodebaseAccess` — the ten tool implementations, Local and Managed, plus `window_content` | `:130-435` |
| Tool dispatch and the turn primitive — `dispatch_tool_call`, `send_turn_and_check_final_answer` | `:532-660` |
| `SpecializedSubagentSession` — the turn loop, synthesis, context-exhaustion landing, and `SubagentRegistry` | `:786-1207` |

The absence of in-file tests is the sharper half of the finding. `dispatch_tool_call` shapes every
tool failure as `format!("{{\"error\": \"{e}\"}}")` (`:588`) — unescaped interpolation into JSON,
and no `is_error` flag — and no test reaches it directly, because it is private and every suite
drives the whole session through a `wiremock` provider. A defect in error shaping is therefore only
visible as a strange downstream symptom, which is how the 2026-09-26 incident presented.

## What would close it

Two extractions clear the budget: `CodebaseAccess` and its ten tool methods to their own module
(~300 lines), and `SpecializedSubagentSession` + `SubagentRegistry` to another (~420), leaving the
vocabulary types and the shared turn primitive in the parent at roughly 480. Anchor with
`tddy-tools restructure anchors`; prove with `restructure check --deep` against a warm index.

**Arithmetic restated at 1,431** (2026-09-26): the same two extractions now leave roughly 590 in
the parent, so a third is needed — `PromptOutcome`/`StopReason`/`ContentBlock`/`SubagentError` and
the `SubagentSession` trait are the obvious one, and they already have two siblings to sit beside
under `src/subagent/` (`transcript.rs`, `turn_request.rs`), which is the directory the earlier
estimate assumed would have to be created.

Separately and more cheaply: the file should gain a `#[cfg(test)]` module covering
`dispatch_tool_call`'s error shaping and `window_content`'s boundaries. That is ordinary work, not
a restructure, and does not need to wait for the split.

**Deliberately not restructured by the changeset that detected it** —
[`2026-09-26-subagent-turn-control-and-honest-tool-failure`](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
records rather than splits, to keep one reviewable PR. That changeset *does* add unit coverage for
the error-shaping seam, because it has to build one.

## Verified by hand

2026-09-26 — Read the file. Confirmed no `#[cfg(test)]` module. Confirmed the four concerns above
are contiguous runs rather than interleaved, so the two named extractions are plausible seams.
Confirmed `dispatch_tool_call`'s error branch at `:588` is the only place a tool failure is turned
into a string and that it carries no error flag. Did **not** run `restructure check`, so the
line estimates for the split are arithmetic, not a proven plan.
