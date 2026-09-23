# 2026-09-23 — `tddy-core` becomes a wiring point over nine crates

**Type:** Refactor

`#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522).

**`tddy-core` held 20,924 production lines on `master` and twelve unrelated responsibilities.**
Every consumer of any one of them compiled all of them, and a five-module cycle tied the largest
together. Every group now lives in a crate of its own. `tddy-core` keeps only `pub use` facades, so
none of the several hundred consumer sites was edited.

| Crate | Takes | Production lines |
|---|---|---:|
| `tddy-core` | `lib.rs`, four facade modules, `ssh_exec` | **54** |
| [`tddy-workflow`](../../../packages/tddy-workflow/README.md) (existing) | + `GoalHints`, `PermissionHint` (Cut 1) | 449 |
| [`tddy-log`](../../../packages/tddy-log/README.md) | `log_backend`, `stdio_safety` | 941 |
| [`tddy-agent-skills`](../../../packages/tddy-agent-skills/README.md) | `agent_skills`, `feature_start_slash` | 549 |
| [`tddy-changeset`](../../../packages/tddy-changeset/README.md) | `changeset` (less Cut 2), `branch_worktree_intent`, the session-metadata modules, `session_lifecycle`, `agent_activity`, `elapsed_format`, `source_path` | 2,486 |
| [`tddy-session-worktree`](../../../packages/tddy-session-worktree/README.md) | `worktree`, `base_sync`, `session_chain`, `git_head` | 1,294 |
| [`tddy-session-actions`](../../../packages/tddy-session-actions/README.md) | `session_actions` (`session_dir`), `session_action_jobs`, `session_action_pipeline` | 1,110 |
| [`tddy-toolcall`](../../../packages/tddy-toolcall/README.md) | `toolcall` | 1,462 |
| [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md) | `backend`, `stream`, `token_accounting`, `claude_argv`, `claude_hooks`, `cursor_hooks`, `spawn_env` | 5,845 |
| [`tddy-workflow-engine`](../../../packages/tddy-workflow-engine/README.md) | `workflow`'s compiled files, plus Cut 2 | 1,867 |
| [`tddy-presenter`](../../../packages/tddy-presenter/README.md) | `presenter`, `post_workflow`, `usage_watcher`, the `cfg(test)` `test_support` | 4,591 |

These counts come from the shape test, `core_facade_shape`. Every receiver is well under the stack's
10,000-line cap. Dependency order, bottom to top: `tddy-workflow` · `tddy-log` · `tddy-agent-skills`
→ `tddy-changeset` → `tddy-session-worktree` · `tddy-session-actions` → `tddy-toolcall` →
`tddy-agent-backend` → `tddy-workflow-engine` → `tddy-presenter` → `tddy-core`. See
[`tddy-core` architecture](../../../packages/tddy-core/docs/architecture.md).

## How the cycle was opened

The strongly connected set was `backend → toolcall → session_actions → changeset → workflow →
backend`. Two small moves opened it:

- **Cut 1:** `PermissionHint` and `GoalHints` (plain data, `workflow/recipe.rs:18-38`) move to
  `tddy-workflow`. They were the only thing the backend took from the workflow. No compiled backend
  file names `WorkflowRecipe`. The one mention was a doc comment, and the one import was in the
  never-compiled `workflow/task.rs`. `WorkflowRecipe` names `CodingBackend`, so the edge runs one
  way, engine → backend.
- **Cut 2:** `start_goal_for_session_continue` moves from `changeset/merge.rs` up to
  `tddy-workflow-engine` (`workflow/session_continue.rs`). It was the only changeset item that named
  `WorkflowRecipe`. `tddy_core::changeset::start_goal_for_session_continue` still resolves through
  `tddy-core`'s `changeset` facade.
- **One retarget:** `toolcall/mod.rs:127` named `ClarificationQuestion` through `crate::backend`,
  and now names `tddy_workflow`, where it already lived. The plan's second one, `error.rs:3`, had
  already been made by #493 when `error.rs` moved to `tddy-session-store`.

Also deleted: the six never-compiled `workflow/{context,graph,hooks,runner,session,task}.rs` files
(1,090 lines; `workflow/mod.rs` already served those paths with inline re-exports of `tddy-graph`),
and the unused `futures` dependency. `agent-client-protocol` and `tokio-util` leave `tddy-core`'s
manifest with the backend, and `jsonschema` with the pipeline.

## How the moves were made

- `git mv` per file. The diff has 152 renames, all at least 74% similar.
- Each crate's root re-export block moved with its modules, and `tddy-core` re-exports every new crate
  whole (`pub use tddy_<crate>::*;`).
- Inside a new crate, modules it does not own are named at their old `crate::` paths by private
  root imports, so moved bodies differ only in `use` lines.
- The other edits: `backend::write_codex_thread_id_file` widens `pub(crate)` → `pub` (the engine
  calls it); the presenter's one call of `start_goal_for_session_continue` names it at
  `crate::workflow`; a few doc-link paths.
- Explicit log targets keep their `tddy_core::…` names. Lines with no explicit target now carry the
  new crates' module paths (see Backlog).

## Premises corrected

1. **"No consumer edited" could not hold for tests that read a moved file by path.** No facade can
   preserve a path like that. Three tests from earlier nodes were retargeted, each keeping its claim:

   | Test | Retarget |
   |---|---|
   | `tddy-integration-tests` `workflow_goal_conditions_acceptance` | its `include_str!` now reads `tddy-presenter/src/presenter/workflow_runner.rs` |
   | `tddy-github` `git_plumbing_shape` (#carve 6, 2 tests) | the session-aware layer stays with the session model, in `tddy-session-worktree/src/worktree.rs`; renamed `the_session_worktree_module_*` |
   | `tddy-core` `session_store_shape` (#carve 7) | `jsonschema` stays with the pipeline, now in `tddy-session-actions`; renamed `jsonschema_stays_with_the_session_action_pipeline` |

   No production code in any consumer changed.
2. **`test_support` left `tddy-core` too.** Its only user is a presenter unit test, so it moved to
   `tddy-presenter`. The PRD had kept it in `tddy-core`.
3. **Two complexity records measured never-compiled code.** See the next section.

## Code issues closed

Re-measured at wrap, then deleted.

| Record | At last measurement | At wrap (2026-09-23) |
|---|---|---|
| `tddy-core`: `cycle-dto-inside-behaviour-module` | 2026-09-19: **3-module SCC**, 2 cycles (`backend ↔ workflow` via the recipe hints; `workflow ↔ changeset`) | **No cycle.** The five modules are in five crates, and Cargo forbids a crate cycle. Normal `tddy-*` dependencies, from each `Cargo.toml`: `tddy-agent-backend` → {`tddy-workflow`, `tddy-session-store`, `tddy-toolcall`}; `tddy-toolcall` → {`tddy-workflow`, `tddy-session-actions`, `tddy-rpc`, `tddy-stdio`}; `tddy-session-actions` → {`tddy-changeset`, `tddy-session-store`, `tddy-task`}; `tddy-changeset` → {`tddy-workflow`, `tddy-session-store`, `tddy-graph`}. None of these reaches `tddy-workflow-engine`. `the_agent_backend_does_not_depend_on_the_workflow_engine` and `the_changeset_crate_does_not_depend_on_the_workflow_engine` pin it |
| `tddy-core`: `complexity-runner-run` (`workflow/runner.rs:41`, `run`) | 2026-09-18: 122 lines, nesting 7, 8 branches, 14 early exits | **File deleted.** `workflow/runner.rs` (163 lines) was never compiled: `workflow/mod.rs` declared `runner` as an inline module re-exporting `tddy_graph::runner`. The finding measured dead code, so it is closed by the deletion, not by a fix. The live `FlowRunner::run` is `tddy-graph/src/runner.rs:39` and has its own record, `packages/tddy-graph/docs/code-issues/complexity-runner-run.md` |
| `tddy-core`: `complexity-task-run` (`workflow/task.rs:224`, `run`) | 2026-09-18: 271 lines, nesting 6, 23 branches, 7 early exits | **File deleted.** `workflow/task.rs` (495 lines) was never compiled, for the same reason. The live `BackendInvokeTask::run` has its own record, now `packages/tddy-workflow-engine/docs/code-issues/complexity-backend-invoke-task-run.md` |

## Code issues moved, opened, unchanged

- **Moved, and unchanged (17).** Each `complexity-*` record moved to its code's new crate with its
  `Location` updated and a `**Moved:**` line. At wrap, each function was re-measured in its new home
  against its `tddy-core` origin on `master`. Every body is line-for-line identical, except
  `run_workflow`, where one line changed (the Cut 2 call path) and the length and structure did not.
  Each record carries a 2026-09-23 row:
  - `tddy-agent-backend`: `run_acp_worker` 184, `invoke_sync` 330, `process_ndjson_stream` 174,
    `run_codex_acp_worker` 244, `process_cursor_stream` 167;
  - `tddy-presenter`: `capture_agent_activity` 101, `poll_workflow` 32, `send_clarification_answers`
    57, `handle_intent` 42, `try_handle_start_slash_line` 49, `handle_elicitation` 134,
    `run_start_goal_without_output_dir` 275, `run_workflow` 382;
  - `tddy-session-worktree`: `setup_worktree_for_session_with_integration_base` 153,
    `setup_worktree_for_session_with_optional_chain_base` 197;
  - `tddy-session-actions`: `stop_session_action_job` 40;
  - `tddy-workflow-engine`: `run` (`BackendInvokeTask`) 272.
- **Opened (8): `oversized-file` records.** The file-length gate flagged these; each is a first
  detection at the file's new home. None grew in this PR. Production lines are counted to the first
  `#[cfg(test)]`, as "before → after":
  - `tddy-agent-backend`: `backend/claude.rs` 767 → 767; `backend/codex_acp.rs` 562 → 562;
    `backend/cursor.rs` 519 → 519; `backend/mod.rs` 878 → 877; `backend/stub.rs` 578 → 578;
  - `tddy-log`: `log_backend.rs` 862 → 862;
  - `tddy-presenter`: `presenter/workflow_runner.rs` 1,015 → 1,015;
  - `tddy-toolcall`: `toolcall/listener.rs` 671 → 671.
- **Not analysed.** None of the nine new crates, nor `tddy-workflow`, has had `/analyze-code-issues`
  (`tddy-tools analyze`) run against it. That needs an instrumented coverage build per crate, which
  this wrap did not pay for. **Pending**: run it per crate so the moved complexity is measured by the
  tool in its new home, not only by hand.

## Backlog

- **Resolved and deleted:**
  - *`backend/` cannot be extracted to a crate while `workflow/recipe.rs` is not a leaf*
    (`2026-09-19-backend-cannot-be-extracted-while-workflow-recipe-is-not-a-leaf`). Its premise
    was false, because no compiled backend file names `WorkflowRecipe`. Cut 1 extracted the backend.
  - *Three files already over budget grew by an import split in `#carve` 5/11*
    (`2026-09-19-three-oversized-files-grew-by-an-import-split`). Its closing condition was that
    the three files be measured again by the records that own them. `backend/claude.rs` 767 and
    `backend/stub.rs` 578 did not grow here, because Cut 1's import changes are line-for-line. Each
    now has its own `oversized-file` record in `tddy-agent-backend`. `tddy-pr-stack`'s
    `pr_stack/mod.rs` (moved by #496) is 457 production lines, under budget.
- **Edited:** `2026-07-01-tddy-core` now names `tddy-toolcall/src/toolcall/listener.rs`. It also
  records that the listener already serves over `tddy-rpc`/`tddy-stdio`, so the item may be done.
- **Added**, one per follow-up the validation found:
  - `2026-09-23-carved-crates-name-their-siblings-through-private-crate-root-shims`
  - `2026-09-23-carved-crate-manifests-repeat-versions-and-tokio-feature-lists`
  - `2026-09-23-tddy-core-build-yaml-does-not-see-the-nine-carved-crates` (pre-existing gap, widened)
  - `2026-09-23-four-intra-doc-links-point-up-the-dependency-order`
  - `2026-09-23-three-carved-crates-dev-depend-back-on-tddy-core`
  - `2026-09-23-implicit-log-targets-moved-with-the-carved-crates`
- **Read, left open:** `2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate`.
  `core_facade_shape.rs` adds one more manifest-parsing copy of that harness.

## Verification

- **Scoped `cargo test`** over the eleven packages (`tddy-core`, `tddy-workflow` and the nine new
  crates): 578 tests, the same count as before the move. 577 passed; the one failure was
  `session_store_shape`, retargeted above. `core_facade_shape` (11 tests) and the
  `core_facade_paths` guard pass. After the retargets, `tddy-core` plus `tddy-github`: 85 passed,
  0 failed.
- **Lint:** `cargo clippy --all-targets -- -D warnings` on those packages is clean, and so is
  `cargo fmt`.
- **CI** on `0fb4fb85` ran 7,132 Rust tests. 7,129 passed, and the 3 failures were the path-reading
  tests retargeted above.
