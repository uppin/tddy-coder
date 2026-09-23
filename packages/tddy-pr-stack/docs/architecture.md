# tddy-pr-stack architecture

## Overview

The PR-stack data model and its git/GitHub operations: every writer of a session's
`Changeset.stack`, the read-side views the orchestrator decides from, the per-PR documents, the
syncs that keep a node's branch and pull request in line with its place in the stack, and the
`pr_stack.PrStackService` handler trait the daemon implements. Product
behaviour is specified in [pr-stacking.md](../../../docs/ft/coder/pr-stacking.md) and
[pr-stack-docs.md](../../../docs/ft/coder/pr-stack-docs.md).

None of it is a workflow recipe. The `pr-stack` recipe (`PrStackRecipe`), its hooks and the
plan→stack bridges live in [`tddy-workflow-recipes`](../../tddy-workflow-recipes/README.md), which
depends on this crate and re-exports every item from its historical path.

## Dependencies

| Crate | What this crate takes from it |
|---|---|
| `tddy-core` | `changeset::{Stack, StackNode, Changeset}`, `read_changeset`, `update_stack_atomic`, `WorkflowError` |
| `tddy-git` | `detect_default_remote_name`, `worktree_path_for_branch`, `local_branch_name_for_remote`, `checked_out_branch_name` |
| `tddy-github` | the PR REST client, `tddy_github::pr_api` (`GithubPrApi`, `GithubPrInsightApi`, `RealGithubPrApi`, `PrState`, `owner_repo_from_remote_url`) |
| `tddy-workflow` | `session_artifacts_root`, which `docs::node_doc_paths` resolves the per-node document root from |
| `tddy-rpc` | `Request`, `Response`, `Status`, `ServiceEntry` — the `rpc` module's handler signatures and transport entry |
| `tddy-service` | the generated `pr_stack` proto types and the `PrStackService` server trait `PrStackServiceImpl` implements. Already reachable through `tddy-github`, so the edge adds no cycle |

External: `log`, `async-trait`, `serde`. Dev-only: `tempfile`, `rstest`.

**It must never depend on `tddy-workflow-recipes`.** That crate depends on this one to re-export it,
so a dependency back is a cycle. The seam is asserted by
`packages/tddy-workflow-recipes/tests/pr_stack_crate_shape.rs`:

| Test | Asserts |
|---|---|
| `the_stack_crate_depends_on_what_the_moved_code_names` | the manifest declares `tddy-core`, `tddy-git`, `tddy-github` |
| `the_stack_crate_does_not_depend_on_the_recipes_it_left` | the manifest does not declare `tddy-workflow-recipes` |
| `the_stack_crate_names_nothing_recipe_side` | no production source here, at any depth, names `plan_pr_stack`, `::writer` or `::parser` |
| `the_recipe_stays_with_the_recipes` | `pr_stack/mod.rs` in `tddy-workflow-recipes` still declares `PrStackRecipe` and `impl WorkflowRecipe for PrStackRecipe` |
| `the_plan_to_stack_bridge_stays_behind` | the same file still declares `pub fn reseed_stack_from_plan_if_unspawned` |

The last two guard the cuts that make the seam acyclic: moving `PrStackRecipe` would make this crate
depend on the workflow machinery, and moving `reseed_stack_from_plan_if_unspawned` would drag
`plan_pr_stack`, which is mutually referenced with the recipe-side `pr_stack` module.

**It must never depend on `tddy-session-lifecycle` either.** The lifecycle crate names
`PrStackHandler` from here so that it can reach the PR-stack family without depending on the crate
that implements it (`tddy-daemon-rpc`); a dependency back would be a cycle.
`packages/tddy-daemon-rpc/tests/rpc_handlers_shape.rs` asserts it:

| Test | Asserts |
|---|---|
| `the_pr_stack_handler_trait_is_defined_by_the_pr_stack_crate` | `pub trait PrStackHandler` is declared in this crate's sources and not in `tddy-session-lifecycle`'s, whose `pr_stack_rpc.rs` is a re-export facade |
| `the_pr_stack_crate_serves_its_own_rpc_family` | the `[dependencies]` table names `tddy-rpc` and `tddy-service`, which `PrStackServiceImpl` is built from |
| `the_pr_stack_crate_depends_on_neither_the_lifecycle_crate_nor_the_recipes` | the `[dependencies]` table names neither `tddy-session-lifecycle` nor `tddy-workflow-recipes` |

## Modules

All six top-level modules are `pub`; `lib.rs` declares them and re-exports `rpc::PR_STACK_SERVICE`
at the crate root.

### `stack_ops/` — the stack operations

Every writer of `Changeset.stack` and the git/GitHub syncs around it. The concern modules are
private; `stack_ops/mod.rs` re-exports every public item, so `tddy_pr_stack::stack_ops::*` is the one
path callers name. `mod.rs` also holds the shared private helpers — `read_stack`,
`append_node_atomic`, `validate_parents`, `reject_if_cyclic`, `message_prefix` — and the test
fixtures the concern modules' tests share.

| Module | Owns |
|---|---|
| `order.rs` | display order: `assign_missing_display_order`, `move_planned_pr_node`; `next_display_order` (`pub(super)`, used by node creation and adoption) |
| `nodes.rs` | `add_planned_pr_node`, `update_planned_pr_node`, `delete_planned_pr_node`, `set_stack_node_parents`, with `AddPlannedPrInput`, `UpdatePlannedPrInput`, `DeletedNode` |
| `repoint.rs` | `repoint_planned_pr_node`; `realign_node_to_effective_base` (`pub(super)`) — the rebase / force-push / PR re-target that `set_stack_node_parents` shares with a repoint |
| `adopt.rs` | `adopt_pr_as_stack_node`, `adopt_pr_into_stack`, `sync_node_to_github_pr`, `AdoptedPrFacts`. An adopted draft PR records `pr_status.phase = "open"` |
| `seed.rs` | base-session seeding: `check_stack_seed_base`, `check_stack_seed_not_self`, `seed_stack_with_base_session`, `SeededBase`, `StackSeedBaseRefusal` |
| `pull_base.rs` | `pull_base_into_node_branch`, `BaseSyncStrategy`, `PullBaseReport` |

`AddPlannedPrInput::child_recipe` is accepted for symmetry with the stack plan's `PlannedPr` and not
stored: `StackNode` has no field to carry it.

### `docs.rs` — per-PR documents

The payload the `write-stack-docs` goal submits (`StackDocsOutput`, `NodeDocs`), its validator
(`validate_stack_docs`, which checks the `REQUIRED_CHANGESET_HEADINGS`), the writer
(`write_stack_docs`), and the path convention (`node_doc_paths` →
`<artifacts root>/prs/<node_id>/{PRD.md,changeset.md}`). Paths are derived from `node_id`, not
recorded on `StackNode`.

### `assess.rs` — the orchestrator's read model

`assemble_views` reads the stack DAG, the child sessions' states and the GitHub PR states into one
`NodeView` per node (`ChildPhase`, `PrLiveStatus`); `effective_base_ref` resolves a node's base as its
nearest unmerged ancestor's branch, else the default branch; `decide_next_action` returns an
`OrchestratorAction`. `AssessTask` is the graph task wrapper. The `pr_*` tools in `tddy-tools` call
these on demand.

### `git_ops.rs` — git helpers for stack syncs

`rebase_onto`, `rebase_branch_onto_ref`, `merge_ref_into_worktree`, `merge_base`, `fetch_ref`,
`force_push_with_lease`, `push_branch`, `commit_all_tracked`, `worktree_is_clean`,
`local_branch_exists`, `head_sha`, `ref_sha`, and `SyncOutcome`. Errors are
`tddy_core::WorkflowError`. `build_integration_ref` is an unimplemented placeholder kept
`pub(crate)`, so it is not part of the public API.

### `pr_insight.rs` — PR-inspection reads

Read-side shaping behind the `pr_read`, `pr_comments` and `pr_search` tools: `read_pr`,
`read_pr_comments`, `search_repository_prs` and their view types. Every function takes
`&dyn GithubPrInsightApi`, so each shape the agent sees is testable against a fake; the tool bodies
in `tddy-tools` only resolve environment and serialize. `pr_number_from_status_url` parses a pull
number out of a node's `pr_status.url` — the one identity a node's PR has — and
`pull_number_for_node` applies it to a stack node.

### `rpc.rs` — the PR-stack RPC family

The one transport-facing module: family P's handler trait, its service adapter and its transport
entry, defined beside the data model they serve.

| Item | What it is |
|---|---|
| `PR_STACK_SERVICE` | the coordinate, `"pr_stack.PrStackService"` |
| `PrStackHandler` | the eight RPCs the host process answers — `add_planned_pr`, `get_pr_status`, `query_branch`, `resolve_stack_base`, `link_stack_node`, `repoint_planned_pr`, `reorder_planned_pr`, `pull_base_into_branch` |
| `PrStackServiceImpl<H>` | the thin `PrStackService` adapter over an `Arc<H: PrStackHandler>` (`new(host)`) |
| `build_pr_stack_entry(service)` | registers `pr_stack.PrStackService` on the RPC transport |

The daemon's implementation is `tddy_daemon_rpc::PrStackRpcHandler`
([tddy-daemon-rpc](../../tddy-daemon-rpc/docs/architecture.md)). `tddy-session-lifecycle` names the
trait for session start's stack-base and stack-link paths, and re-exports the trait, adapter and
entry builder from `pr_stack_rpc` and its crate root; `tddy-workflow-recipes` re-exports
`PR_STACK_SERVICE`.

## What stays in `tddy-workflow-recipes`

| Item | Why it is recipe-side |
|---|---|
| `PrStackRecipe`, its `WorkflowRecipe` and `SessionArtifactManifest` impls, `STATE_STACK_PLANNED`, `STATE_STACK_DOCS_WRITTEN` | the recipe and its states; the manifest impl names `writer::EXPLORATION_BASENAME` |
| `pr_stack/{hooks,bridge}.rs` | recipe hooks and bridges; both name `plan_pr_stack`, and the hooks call `writer::write_exploration_file` |
| `reseed_stack_from_plan_if_unspawned` | calls `plan_pr_stack::{validate_stack_plan, planned_prs_into_stack_nodes}` |
| `orchestrate_pr_stack/bridge.rs` | `seed_orchestrator_stack_from_plan` names `plan_pr_stack` |
| `orchestrate_pr_stack/actions.rs` | `MergeTask` / `RepointTask` call `bridge::{execute_stack_merge, execute_stack_repoint}` |
| `orchestrate_pr_stack/{hooks,transient,pr_actions,internal_status}.rs`, `plan_pr_stack/` | orchestration glue and the plan parser |

The re-exports that keep every historical path resolving:

| In `tddy-workflow-recipes` | Re-exports |
|---|---|
| `pr_stack/mod.rs` | `pub use tddy_pr_stack::docs;` · `pub use tddy_pr_stack::stack_ops::*;` |
| `orchestrate_pr_stack/mod.rs` | `use tddy_pr_stack::assess;` · `pub(crate) use tddy_pr_stack::git_ops;` · `pub use tddy_pr_stack::pr_insight;` |
| `orchestrate_pr_stack/bridge.rs` | `pub use tddy_pr_stack::pr_insight::pr_number_from_status_url;` |

New code names `tddy_pr_stack` directly. Consumers that still reach the data model through
`tddy_workflow_recipes::{pr_stack,orchestrate_pr_stack}` compile every recipe to do so; that
remainder is tracked in
[`squatting-pr-stack-data-model`](../../tddy-workflow-recipes/docs/code-issues/squatting-pr-stack-data-model.md).

## Tests

The crate's unit tests live beside the code (63 in total, in `stack_ops/{nodes,adopt}.rs`,
`docs.rs`, `assess.rs`, `git_ops.rs`, `pr_insight.rs`), sharing fixtures from `stack_ops/mod.rs`'s
test module. The seam tests above live in `tddy-workflow-recipes`, alongside the recipe-side
acceptance suites that exercise these operations through the re-exported paths.
