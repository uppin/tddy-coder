# PRD — tddy-core becomes a wiring point

**Date:** 2026-09-22
**Type:** Refactor — crate extraction, no behaviour change
**Stack:** `#carve` 12/12, on top of `rpc-handlers`
**Packages:** `packages/tddy-core`, `packages/tddy-workflow`, `packages/tddy-session-store`, and nine
new crates (FR3)
**Product area:** [`docs/ft/coder/`](../../ft/coder/)

## Goal

**The developer's target (2026-09-22):** `tddy-core` ends as a **wiring point**. Every group of code
moves to a crate of its own, and `tddy-core` keeps only `pub use` facades, so none of the several
hundred consumer sites is edited. The developer chose to do this **in one PR**. The size cap from
the stack's size-reduction goal applies to every crate that receives code: **at or under 10,000
production lines each**.

## Problem

`tddy-core` holds 20,924 production lines on `master`, after #493 took the storage layer out. Twelve
unrelated responsibilities live in it:
- the agent backends,
- the presenter,
- the workflow engine,
- the changeset model,
- the tool-call protocol,
- session metadata,
- the session-action jobs,
- worktree and base-sync plumbing,
- a log sink,
- agent skills,
- utilities,
- and 1,090 lines of dead files.

Every consumer of any one of them compiles all of them, and a cycle ties five of them together.

## The cycle, and the two cuts that break it

`backend → toolcall → session_actions → changeset → workflow → backend` is the only strongly connected
set. Two small moves open it:

- **Cut 1:** `PermissionHint` and `GoalHints` (`workflow/recipe.rs:18-38`, plain data) move to
  `tddy-workflow`. They are the only thing backend takes from workflow; `WorkflowRecipe` is named by
  no compiled backend file.
- **Cut 2:** `start_goal_for_session_continue` (`changeset/merge.rs:114-168`) moves up to the workflow
  engine. It is the only changeset item that names `WorkflowRecipe`.

There are also two retargets. `error.rs:3` and `toolcall/mod.rs:127` reach `ClarificationQuestion`
through `crate::backend`, but it already lives in `tddy-workflow`.

## What this PR delivers

### FR1 — dead code goes

- The six never-compiled files `workflow/{context,graph,hooks,runner,session,task}.rs` (1,090 lines)
  are deleted. The inline shim modules in `workflow/mod.rs` already serve their paths.
- The unused `futures` dependency is dropped.

### FR2 — the two cuts and two retargets above

### FR3 — ten receivers, each ≤ 10k production lines

| Crate | New? | Takes | Production after |
|---|---|---|---:|
| `tddy-workflow` | existing | `PermissionHint`, `GoalHints` | ~400 |
| `tddy-log` | new | `log_backend`, `stdio_safety` | 927 |
| `tddy-changeset` | new | `changeset` (less Cut 2), `branch_worktree_intent`, `session_lifecycle`, `session_metadata`, `session_agent`, `session_activity`, `session_label`, `session_participant_metadata`, `session_context`, `agent_activity`, `elapsed_format`, `source_path` | ~2,440 |
| `tddy-session-worktree` | new | `worktree`, `base_sync`, `session_chain`, `git_head` | 1,258 |
| `tddy-session-actions` | new | the `session_actions` residue (`session_dir.rs` and its facade), `session_action_jobs`, `session_action_pipeline` | 1,093 |
| `tddy-toolcall` | new | `toolcall` | 1,451 |
| `tddy-agent-backend` | new | `backend`, `stream`, `token_accounting`, `claude_argv`, `claude_hooks`, `cursor_hooks`, `spawn_env` | 5,814 |
| `tddy-agent-skills` | new | `agent_skills`, `feature_start_slash` | 527 |
| `tddy-workflow-engine` | new | `workflow`'s compiled files, plus Cut 2 | 1,848 |
| `tddy-presenter` | new | `presenter`, `post_workflow`, `usage_watcher` | 4,330 |

**Dependency order, bottom to top:**
`tddy-workflow` · `tddy-log` · `tddy-agent-skills` → `tddy-changeset` (over `tddy-session-store`,
`tddy-graph`) → `tddy-session-worktree` (over `tddy-git`) · `tddy-session-actions` → `tddy-toolcall`
→ `tddy-agent-backend` → `tddy-workflow-engine` → `tddy-presenter` → `tddy-core` (facade).

`tddy-session-actions` sits **above** `tddy-changeset`, not inside `tddy-session-store`: the changeset
needs `atomic_file` and `error` from the store, so the jobs could not go into the store without
closing a cycle. That same constraint is why #493 left them in `tddy-core`.

### FR4 — `tddy-core` is facades only

- Every one of the 41 `pub mod` paths and every root-level `pub use` resolves unchanged, re-exported
  from its new crate.
- What stays is `lib.rs`, `ssh_exec` (already a facade over `tddy-git`) and the `cfg(test)`
  `test_support`: about 140 lines.
- The `workflow_decouple_acceptance` guard keeps holding: `plan_prd_path_for_session_dir` is still not
  re-exported.

### FR5 — tests travel with the code

tddy-core's 49 test files move to the crates they exercise, using `move_test_binary_to_crate` from
`#carve` 4. The 19 `complexity-*` code-issue records move with their files.

### FR6 — no behaviour change, no consumer edit

No consumer crate's source changes. **Heavy dependencies leave `tddy-core`'s own manifest with their
modules**: `agent-client-protocol` and `tokio-util` with the backend, `jsonschema` with the pipeline.
A consumer that uses one group only therefore compiles less *once it is repointed*. The repointing
is a later pass and not part of this PR.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `tddy-core` has **≤ 200 production lines**, and every file under its `src/` except `lib.rs`, `ssh_exec.rs` and `test_support.rs` is a pure `pub use` facade |
| AC2 | **Every crate that receives code is ≤ 10,000 production lines** (the ten in FR3) |
| AC3 | The six dead `workflow/*.rs` files are gone and `futures` is not in `tddy-core`'s manifest |
| AC4 | `tddy-agent-backend` does not depend on `tddy-workflow-engine` (Cut 1), and `tddy-changeset` does not depend on it either (Cut 2) |
| AC5 | No receiver depends on `tddy-core`, and the receivers' dependency graph is acyclic in the order above |
| AC6 | `tddy-core`'s normal dependencies no longer include `agent-client-protocol`, `tokio-util` or `jsonschema` except through the new crates |
| AC7 | Every consumer crate compiles **unedited**: no file outside `tddy-core`, the new crates, `tddy-workflow` and `Cargo.toml` / `Cargo.lock` changes |
| AC8 | Every moved test suite passes unedited except for its `use` paths and its crate |
| AC9 | `docs/dev/todo/2026-09-19-backend-cannot-be-extracted-while-workflow-recipe-is-not-a-leaf.md` is **resolved here** (Cut 1) |

## Out of scope

- Repointing consumers from `tddy_core::…` to the new crates, and deleting `tddy-core`. That is the
  "vanish" pass.
- Any behaviour change, and the complexity of what moves.
- `tddy-session-lifecycle`, which is #520's and its successors'.

## Prerequisite — the stack must be on master first

#493 merged to master **after** the `#carve` branches were cut. Everything from #494 up is behind
master and does not contain `tddy-session-store` or `tddy-session-catalog`. A cascade
`/pr-stack-rebase` from #494 through #520 has to run before this node can be greened. The cascade
touches branches owned by four other worktrees, so it is the developer's to start, not this plan's.
