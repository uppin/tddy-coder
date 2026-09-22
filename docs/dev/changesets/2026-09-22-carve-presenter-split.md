# 2026-09-22 — The `Presenter` impl splits along its own state boundaries

**Type:** Refactor

`#carve` 8/9, PR [#495](https://github.com/uppin/tddy-coder/pull/495). Builds on `#carve` 4/9
core-foundations ([#491](https://github.com/uppin/tddy-coder/pull/491)), which grouped
`Presenter`'s 37 fields into five owned sub-structs.

`presenter_impl.rs` was **1,691 production lines**: one struct and one `impl Presenter` holding 46
methods that called each other freely. #491 gave the *state* seams; nothing yet said which methods
belonged to which state. This change partitions the methods along those same five seams and moves
each partition into its own child module. Behaviour-preserving: no signature changes, no
pre-existing method's visibility changes, and every method body outside the two hubs is
whitespace-identical to before.

## Partition

Production lines = lines before the first `#[cfg(test)]`, measured 2026-09-22 after `rustfmt`.

| Module | Prod lines | Role |
|---|---:|---|
| `presenter_impl.rs` (parent) | 219 | the struct; private helpers called from more than one partition (`broadcast`, `broadcast_mode_changed`, `log_activity`, `flush_agent_output_buffer`, `advance_to_next_question`); the `poll_workflow` dispatcher; accessors `state`, `is_done`, `take_workflow_result` |
| `wiring.rs` | 96 | `new`, the `with_*` builders, `set_agent_activity_context` |
| `view_channels.rs` | 179 | `connect_view`, the `handle_intent` dispatcher, select highlighting, the inbox intents, `on_state_change` / `on_goal_started` |
| `activity.rs` | 300 | agent-activity capture and activity-log lines; `on_progress`, `on_worktree_switched`, `on_agent_output` |
| `questions.rs` | 399 | clarification answers, session-document review, `poll_tool_calls`, the `answer_*` handlers |
| `backend_selection.rs` | 185 | backend and recipe-slash selection, the deferred start, `broadcast_error_recovery`, `route_select_answer` |
| `workflow_run.rs` | 469 | start / restart / spawn, start-slash handling, `submit_feature_input`, `continue_with_agent`, `resume_from_error`, `on_workflow_complete` |

70 methods in total: the 46 pre-existing plus 24 handlers split out of the two hubs. The parent
holds three `impl Presenter` blocks (shared helpers, the event dispatcher, the accessors); each
partition module holds one. The parent is 1,125 lines in total because its pre-existing inline
`mod tests` stayed where it was — out of scope.

The resulting layout, the placement rule and the dispatcher pattern are documented in
[`packages/tddy-core/docs/architecture.md`](../../../packages/tddy-core/docs/architecture.md)
§ Presenter.

## The two hubs had to be split

The plan was to move 46 methods without editing any body. That did not survive measurement, because
of how Rust privacy crosses modules: **a private method declared in a child module is visible only
inside that child; one declared in the parent is visible to every child.** So every private method
a hub calls has to live in the hub's own module or in the parent.

`handle_intent` plus every private method it reaches measured **790 lines** on the pre-split file
(exact spans, doc comments included). One module under the 500 budget can hold ~480 lines of
methods, and the parent ~219 under its 250 budget even with every type and free function moved out —
**~699 lines of room against 790 needed, at least 91 short**, before `poll_workflow`'s side is
counted. The only ways through were widening private methods (which AC5 forbids), breaking a budget,
or splitting the hubs.

So `handle_intent` and `poll_workflow` became dispatchers: each match arm moved verbatim
(re-indented; `rustfmt` re-flowed a few lines) into a handler in the partition that owns the state
and private helpers it uses. The one non-textual edit: in `on_progress`, the arm's
`ToolResult { .. } => continue` became `=> return` — the arm was the whole loop iteration, so both
skip the rest of the same event. `AnswerSelect` is the one arm split in two: its backend- and
recipe-selection branches are `route_select_answer` in `backend_selection`, which then calls
`answer_selected_option` in `questions`.

## Visibility

**Only the new handlers are `pub(super)`.** Every pre-existing method keeps its visibility: the 27
that were private before the split are named in a `PRIVATE_TODAY` list and none may be declared with
any `pub` prefix (`pub`, `pub(super)`, `pub(crate)`, `pub(in …)`) anywhere under `presenter_impl/`.
Fields needed nothing — a parent's private fields are visible to its child modules.

## Acceptance tests

`packages/tddy-core/tests/presenter_split_shape.rs`, six tests. Two were revised after the draft
contract was published:

- **AC5** originally banned any `pub(super) fn` in the partition modules. Once the hubs had to be
  split, that ban also forbade the handlers, which are the only way a sibling can reach them. It
  became `every_method_private_before_the_partition_stays_private` over the `PRIVATE_TODAY` list,
  checked for every `pub` spelling. During `/pr-wrap` it was widened to scan every `.rs` file under
  `presenter_impl/` rather than the fixed partition list, and joined by
  `every_method_private_before_the_partition_is_still_declared`, so renaming a method and then
  widening it cannot slip past.
- **AC2** was renamed `the_parent_shrinks_under_budget_but_keeps_the_struct`, with the budget as the
  named `PARENT_BUDGET = 250`.

`every_partition_module_carries_an_inherent_impl` fails a partition that turned methods into free
functions, which would pass a file-existence check while changing every call site.

## Straddle report (AC4)

Method groups that touch more than their own sub-struct plus `state` / `tddy_data_dir` — reported,
not fixed, since #491 owns the fields:

| Method | Group | Sub-structs touched |
|---|---|---|
| `send_clarification_answers` | questions | `questions`, `views`, `workflow` |
| `poll_tool_calls` | questions | `questions`, `views` |
| `prd_body_for_plan_review` | questions | `backend`, `workflow` |
| `reject_session_document`, `answer_text` | questions | `questions`; both write `workflow.answer_tx` |
| `handle_backend_selection_answer`, `handle_recipe_slash_selection_answer`, `show_backend_selection`, `apply_feature_slash_builtin_recipe` | backend selection | `backend`, `questions` |
| `spawn_workflow` | workflow run | `workflow`, `backend` |
| `finish_start_slash_structured_run_if_needed`, `try_handle_start_slash_line` | workflow run | `backend` (`start_slash_structured_run_active`), via `workflow` |
| `on_workflow_complete` | workflow run | `workflow`, `questions`, `backend` |
| `on_goal_started`, `on_state_change` | view channels | `views.critical_state`; `on_goal_started` also `questions` |
| `queue_prompt` | view channels | `questions.awaiting_open_answer`, `workflow.answer_tx` |
| `broadcast_mode_changed` | shared | `views`, `questions` |

The pattern: **questions ↔ backend selection is the weakest of #491's seams** — backend and recipe
selection are presented *as* questions — and the start-slash structured-run flag is backend state
that only the workflow reads. Both are candidates if the grouping is revisited.

## Validation

- `./test -p tddy-core` (scoped): `presenter_split_shape` 6/6 passing. Whole-workspace health is
  CI's, per PR #495's checks.
- `tddy-tools restructure verify --against 310ba24b`: 59 statements lost, 166 gained, exit 1 — every
  entry is a hub arm header replaced by a dispatch arm, a new handler signature, the
  `continue` → `return`, a `rustfmt` re-flow inside a moved arm, the rewritten AC5 test, or doc
  comments and regrouped `use` lists. None is a statement of a pre-existing method outside the hubs.
- `/validate-changes`: 0 critical; 2 warnings (gaps in the straddle report) fixed.
- `/validate-prod-ready`: ready, 0 issues.
- `/validate-tests`: 3 warnings fixed (the AC5 / AC2 revisions above).
- `/analyze-clean-code`: 8/10. `answer_select` renamed `route_select_answer`; module doc comments
  clarified.

Phases A and B were done by a scripted line-span copy from `HEAD`, not `extract_module`: the
assist refuses to lift one member out of an `impl` its siblings call, and partitioning an `impl` in
place has no assist.

## Code issues reconciled

In `packages/tddy-core/docs/code-issues/`:

| Record | Outcome | Final measurement |
|---|---|---|
| `god-object-presenter` | **closed & deleted** | **7 fields** (was 37). Methods: one 46-method `impl` in a 1,691-production-line file → **70 methods** (46 + 24 new handlers) in **nine `impl Presenter` blocks across seven files** (three in the parent, one per partition), largest file **469** production lines, parent **219**. Both close steps done — fields #491, methods #495 |
| `complexity-presenter-impl-handle-intent` | **partially fixed**, renamed `complexity-presenter-impl-view-channels-handle-intent` | 379 lines / nesting 8 → **42 / 2**. Remainder: the extracted `continue_with_agent` (nesting 6) and `resume_from_error` (61 lines), both in `workflow_run.rs` |
| `complexity-presenter-impl-poll-workflow` | **partially fixed** | 236 lines / nesting 7 (brace-depth rescan of the pre-split body) → **32 / 3**. Remainder: the extracted `on_workflow_complete` (66 lines) |
| `complexity-presenter-impl-capture-agent-activity` | moved unchanged, renamed `…-activity-capture-agent-activity` | 101 lines / nesting 4 |
| `complexity-presenter-impl-send-clarification-answers` | moved unchanged, renamed `…-questions-send-clarification-answers` | 56 lines / nesting 7 |
| `complexity-presenter-impl-try-handle-start-slash-line` | moved unchanged, renamed `…-workflow-run-try-handle-start-slash-line` | 49 lines / nesting 7 |

The deleted `god-object-presenter` record's *Verified by hand* note, kept here because it is cited
elsewhere: on 2026-09-15 an earlier pass reported the struct as **44 fields and 79 methods** and was
wrong on both — the fields were eyeballed rather than counted, and the method `grep -c` ran over the
whole file, test module included. Counted on production lines only, it was **37 and 46**.

**Not recorded, pre-existing and unchanged:** `poll_tool_calls` in `questions.rs` — **111 lines,
nesting 7** — breaches both `/analyze-clean-code` thresholds and has no code-issue record. It moved
verbatim with its partition; filing it is left to the next `/analyze-code-issues` run.

No `docs/dev/todo/` entry was resolved: the changeset's two prerequisites were
`2026-09-12-the-two-new-service-rs-files-are-over-budget` (answered — its line between state seams
and size seams is the one this split follows) and `2026-09-09-macro-expansion-as-a-restructure-operation`
(unrelated).
