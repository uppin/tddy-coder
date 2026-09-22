# Changeset: carve-presenter-split

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor
**Stack**: `#carve` 8/9
**PR**: [#495](https://github.com/uppin/tddy-coder/pull/495)

PRD: [`2026-09-15-carve-presenter-split-prd.md`](./2026-09-15-carve-presenter-split-prd.md)

## Initial Discovery

[`2026-09-15-carve-presenter-split-initial-discovery.md`](./2026-09-15-carve-presenter-split-initial-discovery.md)

## Affected Packages

- **`tddy-core`**: [README.md](../../../packages/tddy-core/README.md) — `presenter_impl.rs` becomes a
  directory of six `impl Presenter` blocks.

## Responsibility

- Partition the single 46-method `impl Presenter` into six blocks along `#carve` 4/9's sub-struct
  boundaries, without touching any method body **except the two hubs'** (see Boundaries).
- Move each whole block into its own module.
- Leave the struct, its three state accessors, the private helpers every partition shares, the
  `poll_workflow` dispatcher and the module declarations in `presenter_impl.rs`.

## Boundaries

- Does **not** change the sub-structs — `#carve` 4/9 owns them. A method group that straddles two is
  **reported**, not fixed by moving a field.
- Does **not** touch `presenter/workflow_runner.rs` (1,015 prod lines) or the existing
  `presenter_impl/agent_activity_stamping.rs`.
- Does **not** change any signature, any existing method's visibility, or any behaviour.
- **Does split exactly two method bodies — `handle_intent` and `poll_workflow` — into per-group
  handlers.** Every arm body moves verbatim (re-indented; `rustfmt` re-flows a few lines) into a
  handler living in the partition that owns the state and private helpers it uses; each hub
  becomes a dispatcher. No other body is edited. The one non-textual change: in the `Progress`
  handler, the arm's `ToolResult { .. } => continue` becomes `=> return` — the arm was the whole
  loop iteration, so both skip the rest of the same event.

  *Why this is forced.* A private method declared in a child module is visible only inside that
  child; one declared in the parent is visible to every child. So every private method a hub calls
  must live in the hub's module or in the parent. Measured on the pre-split file (exact spans,
  doc comments included): `handle_intent` plus every private method it reaches is **790 lines**.
  Its module can hold ~480 lines of methods under the 500 budget, and the parent ~219 under the 250
  budget even with every type and free function moved out — **at least 91 lines short** before
  `poll_workflow`'s side is counted. Without splitting the hubs, the only ways through were widening
  private methods (what AC5 forbids) or breaking a budget.
- Does **not** leave `tddy-core`. No cross-crate move, so `move_module_to_crate` is not on its path.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `4/9` core-foundations | `WorkflowRun`, `PendingQuestions`, `ActivityRecorder`, `ViewChannels`, `BackendSelection` as owned sub-structs of `Presenter` | the six-way method partition **is** those boundaries; without the sub-structs there is nothing to partition along | add, rename, re-scope or move a field between sub-structs |
| `1/9`, `3/9` | restructure tooling fixes | **not consumed** — every operation here is in-crate `extract_module`, which already works | touch `tddy-code-restructuring` |
| `5/9`, `6/9`, `7/9` | crate extractions | **not consumed** | touch any of them |

## Draft PR contract

Published first:

**Published** (commit 2): `tests/presenter_split_shape.rs` — five failing assertions pinning the
partition. No new API surface, because every method keeps its name, signature and visibility, so
what lands first is the shape.

**Revised during green — AC5.** The original assertion banned any `pub(super) fn` line in the
partition modules. Once the hubs had to be split (Boundaries), that ban also forbade the new
handlers, which are the only way a sibling can reach them. It is now
`every_method_private_before_the_partition_stays_private`: a named `PRIVATE_TODAY` list of the 27
methods that were private before the split, none of which may be declared with any `pub` prefix
(`pub`, `pub(super)`, `pub(crate)`, `pub(in …)`) in the parent or any partition module. That is the
rule AC5 states — no method becomes more visible than it is today — checked for every spelling
rather than one.

Two of the five are worth naming, because they guard against passing the *letter* of the split while
missing its point: `every_partition_module_carries_an_inherent_impl` fails a partition that turned
methods into free functions (which would satisfy a file-existence check and change every call site in
the crate), and `every_method_private_before_the_partition_stays_private` fails one that gave a
private method any `pub` prefix to reach it across the seam — widening the surface to buy a file
split.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** **no** — the partition is *defined* by `#carve` 4/9's sub-structs. They
must exist as real fields, not just as declared types, for AC4 to mean anything
**Concurrent with:** `#carve` 6/9 `session-store`, 7/9 `telegram`
**Blocks:** nothing

Real dependency edges, as refined by `#carve` 6/9's discovery:

    n1 → n3, n4, n5, n6, n9      n3 → n7, n9      n4 → n6, n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-09-12-the-two-new-service-rs-files-are-over-budget.md](../todo/2026-09-12-the-two-new-service-rs-files-are-over-budget.md) | ℹ **Answered** | Records the file-budget convention and the reason two files were left whole — *"a split for line count alone cuts cohesive units and puts churn on top of a move"*. This node's seams are **state** boundaries, not size boundaries, which is the distinction that entry draws. Not claimed. |
| [2026-09-09-macro-expansion-as-a-restructure-operation.md](../todo/2026-09-09-macro-expansion-as-a-restructure-operation.md) | — Unrelated | No macro in this file's path. |

## State A → State B

### State A

- `presenter_impl.rs` — 1,788 production lines. One struct (37 fields before `#carve` 4/9, seven
  after) and **one `impl Presenter` block from line 152 holding 46 methods**.
- The methods call each other freely: `poll_workflow` → `broadcast`, `log_activity`;
  `handle_intent` → `collect_answers`.
- `presenter/presenter_impl/agent_activity_stamping.rs` (299 lines) already exists.

### State B

- Six modules, one `impl Presenter` block each, partitioned along the sub-struct boundaries.
- `presenter_impl.rs` under 250 production lines.

## Implementation phases

The constraint that shapes every phase: **`extract_module` refuses to lift one member out of an
`impl` its siblings call**, and no ordering fixes it — an `impl` body cannot hold a `mod`. The plan
schema's remedy is to *grow the seam to carry the whole `impl`*.

| Phase | Kind | Work |
|---|---|---|
| **A** | manual | Close and reopen `impl Presenter { … }` six times **in place**, partitioning the 46 methods. No method body is edited — only the block boundaries between them. This is the prep that turns a refused operation into a supported one |
| **B** | mechanical | Move each whole `impl` block to its own file. **Moving a whole `impl` is free of caller churn** — a method is reached through its type — so no call site anywhere changes |
| **C** | manual | Method visibility the split requires. *Corrected:* fields need nothing — `Presenter`'s private fields are declared in `presenter_impl`, and a parent's private items are visible to its child modules. It is **methods** that a sibling cannot see. Only the new hub handlers are `pub(super)`; every pre-existing method keeps its visibility |
| **D** | manual | `README.md` module table; report any method group that straddled two sub-structs |

Phase A is small and mechanical in character but has no operation: partitioning one `impl` into six
is a text edit at five points, and there is no assist for "split this impl".

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (six modules + failing budget/verify assertions)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15, gates delegated)
  - `tests/presenter_split_shape.rs` — **5 failing**. An earlier draft had three of them passing
    *vacuously*, by filtering to files that do not exist; they were tightened to require existence,
    because a test that is green before the work is done is not red.
- [x] Failing unit/integration tests — the same suite; the partition changes no behaviour, so there is nothing else to specify
- [x] Implement production code making tests pass (`/green`)
- [x] Report any method group straddling two sub-structs — see Implementation status
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-core
cargo clippy -p tddy-core -- -D warnings
cargo fmt --all --check
tddy-tools restructure verify --against HEAD
```

`verify --against` is the load-bearing check here: a partition that accidentally edits a method body
is exactly what it catches, statement by statement. **Expected result:** it flags the bodies of
`handle_intent` and `poll_workflow` (split by design, see Boundaries) and nothing else.

**Measured** (2026-09-22, against `310ba24b`): `377591 statements before, 377698 after` — 59 lost,
166 gained, exit 1. Every entry falls in one of these, and none is a statement of a pre-existing
method outside the two hubs:

- the hub arm headers (`UserIntent::X => {`, `WorkflowEvent::X => {`) replaced by one-line dispatch
  arms, and the new handler signatures;
- `ToolResult { .. } => continue` → `=> return` in `on_progress`;
- `rustfmt` re-flows inside the moved arm bodies — five `log::` calls, `self.log_activity(...)`,
  `ExitAction::ContinueWithAgent { session_id: sid }` and one `else if matches!(...)` now fit on one
  line at the shallower indent, so their multi-line fragments (`);`, `log::info!(`) show as lost;
- the rewritten AC5 test; module and handler doc comments; `use` lists regrouped per module.

## Implementation status

Phases A and B were done by hand with a scripted line-span copy from `HEAD`, not with
`extract_module`: every method (other than the two hubs) is copied by its exact span, and a
whitespace-insensitive comparison against `HEAD` confirms all 44 bodies and signatures are
unchanged and no pre-existing method's visibility moved.

### Partition

Production lines (before the first `#[cfg(test)]`), after `rustfmt`:

| Module | Prod lines | Methods (pre-existing) | New handlers |
|---|---:|---|---|
| `presenter_impl.rs` (parent) | 219 | shared private helpers `broadcast`, `broadcast_mode_changed`, `log_activity`, `flush_agent_output_buffer`, `advance_to_next_question`; the `poll_workflow` dispatcher; accessors `state`, `is_done`, `take_workflow_result` | — |
| `wiring.rs` | 96 | `new`, `set_agent_activity_context`, `with_broadcast`, `with_intent_sender`, `with_recipe_resolver`, `with_worktree_dir` | — |
| `view_channels.rs` | 174 | `connect_view`, `handle_intent` (dispatcher), `select_highlight_matches`, `sync_select_highlight` | private: `queue_prompt`, `edit_inbox_item`, `delete_inbox_item`; `pub(super)`: `on_state_change`, `on_goal_started` |
| `activity.rs` | 298 | `agent_activity_head_commit`, `agent_activity_declared_paths`, `capture_agent_activity`, `finalize_agent_line_in_activity_log`, `sync_agent_partial_activity_log` | `pub(super)`: `on_progress`, `on_worktree_switched`, `on_agent_output` |
| `questions.rs` | 399 | `prd_body_for_plan_review`, `approve_plan_from_review_or_viewer`, `clarification_answers_ready`, `send_clarification_answers`, `collect_answers`, `poll_tool_calls` | `pub(super)`: `approve_session_document`, `view_session_document`, `reject_session_document`, `refine_session_document`, `dismiss_viewer`, `answer_selected_option`, `answer_other`, `answer_multi_select`, `answer_text`, `on_clarification_needed`, `on_session_document_approval_needed` |
| `backend_selection.rs` | 184 | `show_backend_selection`, `configure_deferred_workflow_start`, `is_backend_selection_pending`, `broadcast_error_recovery`, `start_workflow_from_pending_if_any`, `apply_deferred_backend_factory`, `handle_backend_selection_answer`, `handle_recipe_slash_selection_answer`, `apply_feature_slash_builtin_recipe`, `recipe_slash_selection_active` | `pub(super)`: `answer_select` |
| `workflow_run.rs` | 469 | `start_workflow`, `restart_workflow`, `spawn_workflow`, `changeset_read_dir`, `finish_start_slash_structured_run_if_needed`, `try_handle_start_slash_line` | `pub(super)`: `submit_feature_input`, `continue_with_agent`, `resume_from_error`, `on_workflow_complete` |

Placement decisions, where the PRD's table was not followable as written:

- **`broadcast_error_recovery` lives in `backend_selection`**, not with the view methods: its only
  caller is `apply_deferred_backend_factory`, and a private method must sit with its caller.
- **`connect_view` moved from wiring to `view_channels`** — it reads `views` and `state` and builds
  the view connection; it is not a builder.
- **`select_highlight_matches` / `sync_select_highlight` live in `view_channels`** beside
  `handle_intent`, their only caller; they touch nothing but `state`.
- **`flush_agent_output_buffer` and `advance_to_next_question` are in the parent**: after the hub
  split each is still called from two partitions — `flush_agent_output_buffer` from `questions`
  and `workflow_run`, `advance_to_next_question` from `questions` and `backend_selection`.
- **`poll_tool_calls` lives in `questions`** — it fills `PendingQuestions` from the tool relay — and
  `workflow_run` would otherwise exceed 500 lines.
- **The `poll_workflow` dispatcher is in the parent.** In `workflow_run` the module measured 504
  lines; the dispatcher is 35.
- **`AnswerSelect`** is the one arm split in two: its backend-selection dispatch (two early
  `return`s) is `answer_select` in `backend_selection`, which then calls `answer_selected_option` in
  `questions` for the clarification answer — the arm's final block, verbatim.
- The `mod tests` in the parent stays there: it reaches only fields and public methods. It gained
  three `use` lines (and one extended) for items the parent no longer imports.

### Straddle report (AC4)

Method groups that touch more than their own sub-struct plus `state` / `tddy_data_dir`. Reported,
not fixed — `#carve` 4/9 owns the fields.

| Method | Group | Sub-structs touched |
|---|---|---|
| `handle_intent` (pre-split) | hub | `workflow`, `questions`, `backend` — now dispatched per group |
| `poll_workflow` (pre-split) | hub | all five, plus `state` — now dispatched per group |
| `send_clarification_answers` | questions | `questions` (via `collect_answers`), `views`, `workflow` |
| `poll_tool_calls` | questions | `questions`, `views` |
| `prd_body_for_plan_review` | questions | `backend`, `workflow` |
| `handle_backend_selection_answer`, `handle_recipe_slash_selection_answer`, `show_backend_selection`, `apply_feature_slash_builtin_recipe` | backend selection | `backend`, `questions` |
| `spawn_workflow` | workflow run | `workflow`, `backend` |
| `finish_start_slash_structured_run_if_needed`, `try_handle_start_slash_line` | workflow run | `backend` (`start_slash_structured_run_active`), via `workflow` |
| `on_goal_started`, `on_state_change` (handlers) | view channels | `views.critical_state`; `on_goal_started` also `questions` |
| `on_workflow_complete` (handler) | workflow run | `workflow`, `questions`, `backend` |
| `broadcast_mode_changed` | shared | `views` (via `broadcast`), `questions` |

The pattern: the **questions ↔ backend-selection** seam is the weakest of 4/9's boundaries — backend
and recipe selection are presented *as* questions — and the start-slash structured-run flag is
backend state that only the workflow reads.
