# oversized-file: subagent.rs

**Location:** `packages/tddy-discovery/src/subagent.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — hand-measured during `/plan-red` Step 2b for the
subagent turn-control changeset
**Metrics:** **1,820 production lines** · budget 500 · **3.6× over** · **no `#[cfg(test)]` module
at all**
**Thresholds breached:** length 1,820 > 500
**Restructure:** required — `extract_module --to_file` × 2, `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | In-file tests | Note |
|---|---|---|---|
| 2026-09-26 | 1,208 | none | first detection |
| 2026-09-26 | 1,324 | none | after the all-tools-failed error path (`ToolDispatch`, `ToolCallTally`) |
| 2026-09-26 | 1,431 | none | after per-call turn budgets, message ids and resume/rewind. Two new sibling modules absorbed what could be moved — `subagent/transcript.rs` (242 production lines, 6 in-file tests) and `subagent/turn_request.rs` (123, 4) — so the growth here is only what has to stay: `PromptOutcome`'s two new fields, `SubagentSession::take_turn`, and the `take_turn`/`run_turn_loop` split |
| 2026-09-27 | 1,687 | none | after tool-argument validation, the generation cap and the glob/grep result window. A third sibling absorbed what could be moved — `subagent/tool_arguments.rs` (275 production lines, its own in-file tests) — so the +256 here is again only what has to stay: `glob_limited`/`grep_limited` and their caps on `CodebaseAccess`, `ToolDispatch::Rejected`, `SubagentConfig::new`/`with_system_prompt`, and the `MaxTokens` stop-reason plumbing through three turn branches |

| 2026-09-27 | 1,820 | none | after the repeated-call guard. A fourth sibling took the logic — `subagent/repeated_calls.rs` (160 production lines) — so the +133 here is wiring only: `ToolDispatch::Repeated` and its arms, the `ToolCallTally` split into `last_dispatch_failure`/`last_refusal`, the session field, `dispatch_bounded` taking `&mut self`, and the ledger reset in `take_turn` |
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
| 2026-09-28 | 1,850 | none | after PR #553's result summaries. The extraction itself went to a new sibling — `subagent/result_summary.rs` (types + per-tool extraction, with its own 11 in-file tests) — so the +18 here is wiring only: `ToolDispatch::summary`, the `push_tool_result` append site in `run_one_turn`, and the descriptor field read |
| 2026-09-28 | 1,912 | none | after PR #554's Grep context lines. The window computation went to a new sibling — `subagent/grep_context.rs` (argument reading, entry shapes, the Local path's windows) — so the +62 here is the `grep_with_context` seam (contract-planned), the `scan_local` extraction shared with `grep_limited`, and the GREP dispatch arm threading the pair |
| 2026-09-28 | 1,985 | none | after PR #556's yield conditions. The predicate machinery went to a new sibling — `subagent/yield_condition.rs` (types, evaluation, validation, its own 7 in-file tests) — so the +73 here is the planned seams only: the `run_one_turn` loop check with its early return, the `run_turn_loop`/`take_turn` threading, and the turn-entry validation |
| 2026-09-28 | 2,001 | +16 more in PR #557 (`#subagent-control` 5/5): the resume path's replacement validation (above the rewind) and append — the planned in-place seam |
| 2026-10-01 | 2,004 | **worse** by 3 (wc, master 2,001) in PR #560 (`#agent-worktree` 1/4): wiring only — the `mod worktree_change;` line, the re-export of `WorktreeChange`/`FileCounts`/`LineCounts` and the `worktree_change::of(&dispatch)` call at the append site; the reading itself lives in `subagent/worktree_change.rs` |
| 2026-10-01 | gate: 2,004 → 2,050 | **worse** by 46 in PR #561 (`#agent-worktree` 2/4): the rewind-reset wiring that has to stay in this file — `SubagentConfig::worktree_reset` / `with_worktree_reset`, the session field and `resetting_worktree_through`, the reset step in `take_turn`, `PromptOutcome::worktree_reset`, and the boxed `TurnStep::FinalAnswer`. The port, target and reset types already live in the new `subagent/worktree_reset.rs`; `commit_kept_by` in `transcript.rs`. **Split deferred, not skipped**: #560 (parent) also touches this file, so a decomposition here would turn the stack's diffs into conflicts — do it on a follow-up branch after `#agent-worktree` lands. |
