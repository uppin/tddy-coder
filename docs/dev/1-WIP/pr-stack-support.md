# PR stacking support

Adds a parent orchestrating session that coordinates a DAG of child PR sessions via two new workflow
recipes (`plan-pr-stack`, `orchestrate-pr-stack`) and enriched `changeset.yaml` fields.

## tddy-core

- `Stack` / `StackNode` structs added to `changeset.rs`; two new optional `Changeset` fields:
  `stack: Option<Stack>` (orchestrator-only DAG) and `orchestrator_session_id: Option<String>`
  (child→orchestrator back-reference, distinct from `previous_session_id`).
- Helpers on `Stack`: `topo_order`, `effective_base_refs`, `node`.
- Helper on `StackNode`: `is_skipped` (true when `pr_status.phase == "merged"`).
- Read/write helpers (temp+rename atomicity): `update_stack_atomic`,
  `link_stack_node_to_child_session`, `sync_stack_node_from_child`.
- `session_chain.rs` — new transport-agnostic `spawn_chain_child_worktree` (lifted from Telegram
  session control; Telegram wrapper stays thin).
- `lib.rs` — re-exports all new public symbols.

## tddy-workflow-recipes

- `plan_pr_stack/` — new module: `mod.rs` (recipe + manifest + parser + `planned_prs_into_stack_nodes`
  mapper + state table), `hooks.rs`, `prompt.rs`. CLI name `plan-pr-stack`.
  Pipeline: `analyze-stack → write-stack-plan → end`. Artifacts: `stack-plan.yaml` +
  `pr-stack-plan.md`. `uses_primary_session_document = false`.
- `orchestrate_pr_stack/` — new module: `mod.rs` (recipe + manifest + graph),
  `assess.rs` (idempotent decision loop; `decide_next_action`; `NodeView` / `ChildPhase` /
  `OrchestratorAction` types), `actions.rs` (spawn / merge / repoint custom Task impls),
  `git_ops.rs` (`rebase --onto` + force-with-lease + merge-base guard + `git rerere`),
  `github.rs` (`GithubPrApi` trait + `RealGithubPrApi`; `get_open_pr`, `merge_pr`,
  `patch_pr_base`, `create_pr`), `hooks.rs` (rollup writer; `on_error` → `Failed`),
  `transient.rs` (crash-safe `StackOpJournal`; `recover_in_flight_stack_op`).
  CLI name `orchestrate-pr-stack`. All goals: `goal_requires_tddy_tools_submit = false`.
- `github_rest_common.rs` — shared `curl_github_patch_json` + `curl_github_post_json` helpers
  (alongside existing GET/PUT helpers from `merge_pr/github.rs`).
- `recipe_resolve.rs` — resolution arms for `"plan-pr-stack"` and `"orchestrate-pr-stack"`.
- `approval_policy.rs` — both CLI names added to the supported-names list and the
  skip-session-document-approval table.
- `lib.rs` — `pub mod` + `pub use` for both recipes.

## tddy-coder

- `run.rs` — two new CLI flags: `--stack-parent <orchestrator-session-id>` (sets child
  `orchestrator_session_id`) and `--stack-base <base-session-id>` (sets `previous_session_id` +
  integration base ref, calls `spawn_chain_child_worktree`).

## tddy-daemon

- `telegram_session_control.rs` — existing
  `merge_chain_integration_base_with_explicit_operator_overrides` made a thin wrapper over
  `tddy-core::session_chain::spawn_chain_child_worktree`.

## Note for docs/ft/coder/workflow-recipes.md

Update the recipe list to include `plan-pr-stack` and `orchestrate-pr-stack` once this changeset
lands (per normal changeset → docs merge).
