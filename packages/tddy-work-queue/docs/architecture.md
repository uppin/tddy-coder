# tddy-pr-stack architecture

## Overview

The PR-stack data model and its git/GitHub operations: every writer of a session's
PR stack (held in its project's stack store), the read-side views the orchestrator decides from, the per-PR documents, the
syncs that keep a node's branch and pull request in line with its place in the stack, and the
`pr_stack.PrStackService` handler trait the daemon implements. Product
behaviour is specified in [pr-stacking.md](../../../docs/ft/coder/pr-stacking.md) and
[pr-stack-docs.md](../../../docs/ft/coder/pr-stack-docs.md).

None of it is a workflow recipe, and no workflow recipe drives a stack: stacks are planned and driven
through the project stack view and the daemon's `pr_*` tools.

## Dependencies

| Crate | What this crate takes from it |
|---|---|
| `tddy-core` | `changeset::{Stack, StackNode, Changeset}`, `read_changeset`, `read_session_stack`, `update_stack_atomic`, `project_stack_store::ProjectStackStore`, `WorkflowError` |
| `tddy-git` | `detect_default_remote_name`, `worktree_path_for_branch`, `local_branch_name_for_remote`, `checked_out_branch_name` |
| `tddy-github` | the PR REST client, `tddy_github::pr_api` (`GithubPrApi`, `GithubPrInsightApi`, `RealGithubPrApi`, `PrState`, `owner_repo_from_remote_url`) |
| `tddy-workflow` | `session_artifacts_root`, which `docs::node_doc_paths` resolves the per-node document root from |
| `tddy-rpc` | `Request`, `Response`, `Status`, `ServiceEntry` — the `rpc` module's handler signatures and transport entry |
| `tddy-service` | the generated `pr_stack` proto types and the `PrStackService` server trait `PrStackServiceImpl` implements. Already reachable through `tddy-github`, so the edge adds no cycle |

External: `log`, `async-trait`, `serde`, `serde_json` (the merge journal), `uuid` (a journalled operation's id). Dev-only: `tempfile`, `rstest`.

**It must never depend on `tddy-workflow-recipes`.** That crate's acceptance suites still exercise
this one (a dev-dependency), and `recipe_catalogue_acceptance::the_recipes_crate_holds_no_stack_module`
asserts the recipes crate holds no stack module of its own.

**It must never depend on `tddy-session-lifecycle`.** The lifecycle crate names
`PrStackHandler` from here so that it can reach the PR-stack family without depending on the crate
that implements it (`tddy-daemon-rpc`); a dependency back would be a cycle.
`packages/tddy-daemon-rpc/tests/rpc_handlers_shape.rs` asserts it:

| Test | Asserts |
|---|---|
| `the_pr_stack_handler_trait_is_defined_by_the_pr_stack_crate` | `pub trait PrStackHandler` is declared in this crate's sources and not in `tddy-session-lifecycle`'s, whose `pr_stack_rpc.rs` is a re-export facade |
| `the_pr_stack_crate_serves_its_own_rpc_family` | the `[dependencies]` table names `tddy-rpc` and `tddy-service`, which `PrStackServiceImpl` is built from |
| `the_pr_stack_crate_depends_on_neither_the_lifecycle_crate_nor_the_recipes` | the `[dependencies]` table names neither `tddy-session-lifecycle` nor `tddy-workflow-recipes` |

## Modules

All seven top-level modules are `pub`; `lib.rs` declares them and re-exports `rpc::PR_STACK_SERVICE`
at the crate root.

### `stack_ops/` — the stack operations

Every writer of an orchestrator's stack and the git/GitHub syncs around it. Each reads through
`read_session_stack` and writes through `update_stack_atomic`, both of which resolve the session's
project stack record. The concern modules are
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
| `pull_base.rs` | `pull_base_into_node_branch`, `BaseSyncStrategy`, `PullBaseReport` |

`AddPlannedPrInput::child_recipe` is accepted and not stored: `StackNode` has no field to carry it.

### `docs.rs` — per-PR documents

A pass of document pairs (`StackDocsOutput`, `NodeDocs`), its validator (`validate_stack_docs`, which
checks the `REQUIRED_CHANGESET_HEADINGS`), the writer (`write_stack_docs`), the stack-plan basename
(`PR_STACK_PLAN_MD_BASENAME`), and the path convention (`node_doc_paths` →
`<artifacts root>/prs/<node_id>/{PRD.md,changeset.md}`). Paths are derived from `node_id`, not
recorded on `StackNode`.

### `assess.rs` — the orchestrator's read model

`assemble_views` reads the stack DAG, the child sessions' states and the GitHub PR states into one
`NodeView` per node (`ChildPhase`, `PrLiveStatus`); `effective_base_ref` resolves a node's base as its
nearest unmerged ancestor's branch, else the default branch; `decide_next_action` returns an
`OrchestratorAction`. The daemon's `pr_stack_status` calls `assemble_views` and `effective_base_ref`.

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

### `stack_migration.rs` — the startup sweep

`migrate_session_stacks(sessions_base, projects_dir) -> MigrationReport` moves every stack still
carried in a session's `changeset.yaml` (`Changeset.stack`, the pre-store field) into its project's
stack store, then names the record in `Changeset.stack_id` and drops the copy. It is the only reader
of that field. Record before id, so a rerun after an interruption finds the record by
`created_by_session_id`; a second run changes nothing. `MigrationReport { migrated_session_ids,
sessions_without_project, failures: Vec<MigrationFailure { session_id, message }> }`, each list
sorted; the sweep logs a summary and each session it left alone or could not move. `tddy-daemon`
calls it at startup (`runtime/mount_host.rs`) over its data directory; the body lives here because
the daemon crate is wiring only. See [pr-stacking.md § Where a stack lives](../../../docs/ft/coder/pr-stacking.md#where-a-stack-lives).

### `internal_status.rs`, `op_journal.rs`, `merge_repoint.rs`

`internal_status`: `derive_internal_status` / `reconcile_internal_status` — a node's action-needed
signal, which the daemon's `pr_stack_status` writes back. `op_journal`: `StackOpJournal`,
`MergePhase` and the journal file's read / write / delete, plus `recover_in_flight_stack_op`.
`merge_repoint`: `execute_stack_merge` (journalled merge of one node) and `execute_stack_repoint`
(rebase and PR-base patch of each dependent). The daemon's `pr_merge` journals through
`StackOpJournal` beside the stack; `recover_in_flight_stack_op`, `execute_stack_merge` and
`execute_stack_repoint` have no production caller today (see the backlog entry *Stack helpers with no
production caller*).

## Tests

The crate's unit tests live beside the code (`stack_ops/{nodes,adopt}.rs`, `docs.rs`, `assess.rs`,
`git_ops.rs`, `pr_insight.rs`, `rpc.rs`), sharing fixtures from `stack_ops/mod.rs`'s test module.
Acceptance suites that exercise these operations still live in `tddy-workflow-recipes/tests/`, naming
`tddy_pr_stack` directly.

## GitHub credential

Every function here that reaches GitHub takes its `GithubPrApi` from the caller; the daemon builds it
with the project account's token. Nothing here reads a token from the environment.
