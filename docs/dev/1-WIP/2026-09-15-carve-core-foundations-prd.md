# PRD — `tddy-core`'s shared vocabulary leaves, and two god-files gain seams

**Date:** 2026-09-15
**Stack:** `#carve` 4/9
**Packages:** `packages/tddy-core`, `packages/tddy-workflow`
**Product area:** [`docs/ft/coder`](../../ft/coder/)

## Problem

`tddy-core` is the workspace's god-crate: **35 crates depend on it**, and it holds seven unrelated
concerns in one tree. Discovery found the reason it cannot be decomposed is smaller and more specific
than its size suggests.

### Every cycle in `tddy-core` is a DTO living inside a behaviour module

Six modules — `backend`, `stream`, `toolcall`, `workflow`, `changeset`, `presenter` — form a
strongly-connected component. But **each edge is one to four symbols wide**, and every one of them is
a plain data type that has no business living inside the module that happens to define it:

| Cycle | The entire edge |
|---|---|
| `backend ↔ stream` | `stream/mod.rs:9` — `use crate::backend::{ClarificationQuestion, QuestionOption};` |
| `backend ↔ toolcall` | `toolcall/client_wire.rs` — `crate::backend::QuestionOption` |
| `presenter ↔ workflow` | `workflow/` needs `crate::presenter::WorkflowEvent`, twice |
| `backend ↔ workflow` | `backend/mod.rs:230-231` — a **re-export facade** for `GoalId` and the recipe trio |
| `changeset ↔ workflow` | `workflow/` needs `crate::changeset::Changeset`, once |

The DTOs themselves are leaves. `workflow/ids.rs` (107 lines) and `presenter/events.rs` (40 lines)
have **zero `crate::` dependencies**; `ClarificationQuestion` and `QuestionOption`
(`backend/mod.rs:510` and `:523`) are plain serde structs.

And there is already somewhere for them to go: **`tddy-workflow` is an existing 384-line crate with
no `tddy-*` dependencies at all**, which both `tddy-core` and `tddy-workflow-recipes` already depend
on.

### Two files carry no internal boundaries

- **`changeset.rs` — 964 production lines.** Two unrelated data models in one file: the **PR-stack**
  model (`Stack`, `StackNode` and their impls, lines 44–277) and the **session** model (`Changeset`,
  `ChangesetState`, `ChangesetWorkflow`, 278 onward), plus file I/O and context-merge functions.
  `#carve` 9/9 needs `Stack` as its own module.
- **`presenter/presenter_impl.rs` — 1,788 production lines.** One struct with **37 fields** and one
  `impl` with **46 methods**. The field clusters are already named by the struct's own doc comments;
  they are simply not expressed in the type system.

## What this PR delivers

### FR1 — the shared vocabulary moves to `tddy-workflow`

| Symbol | From | Lines |
|---|---|---:|
| `GoalId`, `WorkflowState` | `tddy-core/src/workflow/ids.rs` | 107 |
| `WorkflowEvent` | `tddy-core/src/presenter/events.rs` | 40 |
| `ClarificationQuestion`, `QuestionOption` | `tddy-core/src/backend/mod.rs:510-533` | ~25 |

Every old path re-exports, so **no consumer of `tddy-core` is edited**. This breaks
`backend ↔ stream`, `backend ↔ toolcall` and `presenter ↔ workflow` outright, and thins
`backend ↔ workflow` to the recipe trio.

`backend/mod.rs:230-231`'s re-export facade is retired in favour of the real paths.

### FR2 — `changeset.rs` gains seams

```
changeset/stack.rs    Stack, StackNode, their impls, update_stack_atomic,
                      link_stack_node_to_child_session, resolve_stack_node_branch,
                      read_stack_with_resolved_branches, sync_stack_node_from_child
changeset/model.rs    Changeset, SessionEntry, ChangesetState, StateTransition,
                      DiscoveryData, RelevantCode, TestInfrastructure, ChangesetWorkflow,
                      GithubPrStatus, PrInternalStatus, impl Changeset, Default
changeset/io.rs       read_changeset, write_changeset, write_changeset_atomic,
                      ensure_changeset_recipe
changeset/merge.rs    merge_persisted_workflow_into_context, merge_post_workflow_into_context,
                      start_goal_for_session_continue, resolve_model, update_state,
                      clarification_qa_from_backend, append_session_and_update_state
```

`changeset.rs` keeps `ClarificationQa`, `QuestionOptionForQa`, `BranchWorktreeIntent` and a glob
facade, so every existing `changeset::` path resolves.

### FR3 — `Presenter`'s 37 fields become five owned sub-structs

Grouped as the struct's own comments already group them:

| Sub-struct | Fields |
|---|---|
| `WorkflowRun` | `workflow_event_rx`, `answer_tx`, `workflow_backend`, `workflow_output_dir`, `workflow_session_dir`, `workflow_conversation_output`, `workflow_debug_output`, `workflow_debug`, `workflow_result`, `workflow_handle`, `workflow_socket_path`, `workflow_worktree_dir` |
| `PendingQuestions` | `pending_questions`, `current_question_index`, `collected_answers`, `awaiting_open_answer` |
| `ActivityRecorder` | `agent_activity_dir`, `agent_activity_worktree`, `agent_activity_source`, `agent_activity_pending`, `agent_output_buffer`, `agent_output_partial_row_active` |
| `ViewChannels` | `broadcast_tx`, `intent_tx`, `tool_call_rx`, `pending_tool_call_response`, `critical_state` |
| `BackendSelection` | `backend_selection_pending`, `deferred_backend_factory`, `pending_workflow_start`, `deferred_cli_model`, `workflow_recipe`, `recipe_slash_selection_pending`, `recipe_resolver`, `start_slash_structured_run_active` |

`state` and `tddy_data_dir` stay directly on `Presenter`.

**This is the node's one behaviour-affecting change**, and it is behaviour-*preserving*: every field
access site moves, so the existing tests are what prove it. It does **not** split the methods — that
is `#carve` 8/9, which follows these boundaries.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `stream`, `toolcall` and `presenter` no longer name `crate::backend` or `crate::presenter` for a DTO; the three cycles are gone |
| AC2 | Every pre-existing path for the five moved symbols still resolves — **no consumer crate is edited** |
| AC3 | `backend/mod.rs` no longer re-exports `GoalId`/`GoalHints`/`PermissionHint`/`WorkflowRecipe` |
| AC4 | `changeset.rs` is under 200 production lines; no `changeset/` module exceeds 400 |
| AC5 | `Stack` and `StackNode` are reachable at `tddy_core::changeset::stack::` **and** at their old paths |
| AC6 | `Presenter` has seven fields: five sub-structs plus `state` and `tddy_data_dir` |
| AC7 | No sub-struct field is `pub` outside the presenter module — the grouping reduces reach, not just line count |
| AC8 | `./test -p tddy-core` passes with the same test count as baseline |

## Out of scope

- **Splitting `Presenter`'s 46 methods** — `#carve` 8/9, which consumes FR3's sub-structs.
- **Extracting `backend/` to its own crate.** It remains in the SCC via the recipe trio, and
  `workflow/recipe.rs` is **not** a leaf (it needs `backend`, `changeset::Changeset`,
  `presenter::WorkflowEvent`, and four `workflow::` submodules). Recorded as a todo, not attempted.
- `worktree.rs` — `#carve` 5/9.
- `session_actions/`, `session_catalog/` — `#carve` 6/9.

## Correction to earlier measurement

The whole-work discovery recorded `Presenter` as **44 fields / 79 methods**. Both were over-counted
by grepping the whole file including its test module. Measured against production lines only:
**37 fields, 46 methods**. The 1,788-line figure is unchanged and correct.
