# PRD — the `Presenter` impl splits along its own state boundaries

**Date:** 2026-09-15
**Stack:** `#carve` 8/9
**Package:** `packages/tddy-core`
**Product area:** [`docs/ft/coder`](../../ft/coder/)

## Problem

`tddy-core/src/presenter/presenter_impl.rs` is **1,788 production lines**: one struct and **one
`impl Presenter` block holding 46 methods**, spanning from line 152 to the end of the production
region.

`#carve` 4/9 groups the struct's 37 fields into five owned sub-structs — `WorkflowRun`,
`PendingQuestions`, `ActivityRecorder`, `ViewChannels`, `BackendSelection`. That fixes the *state*
coupling. The methods are still one undifferentiated block, so the file stays 1,788 lines and nothing
expresses which methods belong to which state.

The 46 methods sort cleanly onto those boundaries:

| Group | Methods |
|---|---|
| wiring | `new`, `set_agent_activity_context`, `with_broadcast`, `with_intent_sender`, `with_recipe_resolver`, `with_worktree_dir`, `connect_view` |
| view channels | `broadcast`, `broadcast_mode_changed`, `broadcast_error_recovery` |
| activity recorder | `agent_activity_head_commit`, `agent_activity_declared_paths`, `capture_agent_activity`, `flush_agent_output_buffer`, `finalize_agent_line_in_activity_log`, `sync_agent_partial_activity_log`, `log_activity` |
| pending questions | `select_highlight_matches`, `sync_select_highlight`, `handle_intent`, `clarification_answers_ready`, `send_clarification_answers`, `collect_answers`, `advance_to_next_question`, `prd_body_for_plan_review`, `approve_plan_from_review_or_viewer` |
| backend selection | `show_backend_selection`, `configure_deferred_workflow_start`, `is_backend_selection_pending`, `start_workflow_from_pending_if_any`, `apply_deferred_backend_factory`, `handle_backend_selection_answer`, `handle_recipe_slash_selection_answer`, `apply_feature_slash_builtin_recipe`, `recipe_slash_selection_active` |
| workflow run | `poll_tool_calls`, `poll_workflow`, `start_workflow`, `restart_workflow`, `spawn_workflow`, `changeset_read_dir`, `finish_start_slash_structured_run_if_needed`, `try_handle_start_slash_line` |
| root | `state`, `is_done`, `take_workflow_result` |

## The constraint that shapes this node

**`extract_module` cannot lift a method out of an `impl` whose siblings call it.** The plan schema
records three geometries and says only one is refused:

| Geometry | Outcome |
|---|---|
| A whole `impl` moves; the parent calls its methods | **Succeeds** — a method is reached through its type |
| A path-reached item moves; the parent still names it | **Succeeds** |
| **One member is lifted out of an `impl` while a sibling in that same `impl` calls it** | **Refused** |

The third is exactly this file: all 46 methods are in one `impl`, and they call each other constantly
(`poll_workflow` calls `broadcast` and `log_activity`; `handle_intent` calls `collect_answers`). The
schema is explicit that **no ordering fixes it** — an `impl` body cannot hold a `mod`, so a sibling
can be moved neither out of the way first nor after.

The schema's own remedy applies: *"Grow the seam to carry the whole `impl`."* Rust allows a type's
inherent methods to be spread across **several `impl` blocks in different modules of the same crate**.
So the node's shape is forced:

1. **By hand**, close and reopen `impl Presenter { … }` six times in place, partitioning the 46
   methods. No method body is touched — only the block boundaries between them.
2. **Mechanically**, `extract_module --to_file` each of the six now-whole `impl` blocks. Moving a
   whole `impl` is the cheapest restructuring move Rust has, and **free of caller churn**.

## What this PR delivers

`presenter/presenter_impl.rs` becomes `presenter/presenter_impl/` with one module per state
boundary — `wiring.rs`, `view_channels.rs`, `activity.rs`, `questions.rs`, `backend_selection.rs`,
`workflow_run.rs` — each holding one `impl Presenter` block. The struct definition and the three
state accessors stay in `presenter_impl.rs`.

`presenter/presenter_impl/agent_activity_stamping.rs` (299 lines) already exists and is unchanged.

**Behaviour-preserving.** No signature changes, no logic edits.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | Six modules, each holding exactly one `impl Presenter` block |
| AC2 | `presenter_impl.rs` is under 250 production lines — the struct, its three accessors, and the module declarations |
| AC3 | No module exceeds 500 production lines |
| AC4 | Each module's methods touch only its own sub-struct plus `state` / `tddy_data_dir`, or the deviation is named in the changeset |
| AC5 | No method signature changes, and no method becomes more visible than it is today |
| AC6 | `restructure verify --against HEAD` reports no moved logic |
| AC7 | `./test -p tddy-core` passes at baseline test count |

## Out of scope

- **Changing the sub-structs.** They are `#carve` 4/9's surface. If a method group needs a field this
  node cannot reach, that is a finding to report upward, not a field to move.
- Splitting `presenter/workflow_runner.rs` (1,015 prod lines) — a separate file, not this impl.
- Any behaviour change, any new test beyond what moves with its code.

## Why AC4 is worded as it is

The five-way field grouping is a hypothesis about cohesion that `#carve` 4/9 makes and this node is
the first real test of. If a method group turns out to straddle two sub-structs, that is **evidence
about the grouping**, not a failure of this node — so the criterion asks for the deviation to be
**named**, not for it to be absent.
