# PRD — git plumbing and the GitHub REST client find their crates

**Date:** 2026-09-15
**Stack:** `#carve` 5/9
**Packages:** `packages/tddy-core`, `packages/tddy-git` (new), `packages/tddy-workflow-recipes`, `packages/tddy-github`
**Product area:** [`docs/ft/coder`](../../ft/coder/)

## Problem

Two bodies of low-level plumbing sit in crates that have nothing to do with them, and both are
consumed by `#carve` 9/9 (`pr-stack-crate`).

### 1. `tddy-core/src/worktree.rs` is 1,606 production lines of git, with 2 session-aware functions

Forty-eight free functions wrapping `git`: `create_worktree`, `git_rev_parse`,
`push_new_branch_to_remote`, `list_worktrees`, `local_branch_name`, `validate_integration_base_ref`,
`fetch_origin_master`, `detect_default_remote_name`, `remote_branch_ref_sha`,
`first_free_suffixed_branch_name`, `remove_worktree`, `list_recent_remote_branches`, and so on.

**Only two of them know what a session is.** The file's single `crate::changeset` import
(`worktree.rs:11`) is consumed at lines 828–962 and 1178–1273 — inside
`setup_worktree_for_session_with_integration_base` and
`setup_worktree_for_session_with_optional_chain_base`. Everything else is pure git and depends on
nothing in `tddy-core`.

So ~1,200 lines of general-purpose git plumbing are locked inside the workspace's god-crate, where
**35 dependents** compile it whether they touch git or not.

### 2. A GitHub REST client lives in a workflow-recipes crate, next to a `tddy-github` that has none

| File | Lines | Depends on |
|---|---:|---|
| `src/github_rest_common.rs` | 338 | **nothing** — a pure leaf |
| `src/github_pr.rs` | 494 | `crate::github_rest_common` only |
| `src/orchestrate_pr_stack/github.rs` | 1,292 | `crate::github_rest_common::github_token_from_env` only |

**2,124 lines of GitHub REST**, and not one of them touches `tddy_core`, the recipe machinery, or
`pr_stack`. Meanwhile `tddy-github` exists — 1,340 lines of OAuth, session tokens and a token store —
with **no PR surface at all**, and `tddy-workflow-recipes` does not depend on it.

`github_pr` is already consumed **outside** the crate, by `tddy-tools/src/server.rs`.

## What this PR delivers

### FR1 — a `tddy-git` crate

The pure-git portion of `worktree.rs` moves to a new `tddy-git`. `tddy-core` keeps the session-aware
layer — the two `setup_worktree_for_session*` functions, their `setup_worktree_for_session` wrapper,
`resolve_persisted_worktree_integration_base_for_session`, and `setup_worktree_for_session_over_ssh` —
which call into `tddy-git` for every git operation.

`tddy-core::worktree` keeps a facade, so **no consumer is edited**.

### FR2 — the GitHub REST client moves to `tddy-github`

All three files move, in **leaf-first order**: `github_rest_common`, then `github_pr`, then
`orchestrate_pr_stack/github.rs`. `tddy-workflow-recipes` gains a `tddy-github` dependency and keeps
facades at the old paths, so `tddy-tools/src/server.rs` is not edited.

### FR3 — no behaviour changes

Both are pure mechanical extractions. `restructure verify --against HEAD` must report every changed
statement as a move, a re-point or a facade line.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `tddy-git` exists, depends on no `tddy-*` crate, and holds no function naming `Changeset` |
| AC2 | `tddy-core/src/worktree.rs` is under 450 production lines and holds only the session-aware layer |
| AC3 | Every pre-existing `tddy_core::worktree::…` path resolves — no consumer edited |
| AC4 | `tddy-github` exposes `github_pr`, `github_rest_common` and the PR-state API from `orchestrate_pr_stack/github.rs` |
| AC5 | Every pre-existing `tddy_workflow_recipes::github_pr::…` path resolves — `tddy-tools/src/server.rs` is not edited |
| AC6 | `tddy-github` still depends only on `tddy-rpc` and `tddy-service` plus what the moved code needs; no dependency on `tddy-core` or `tddy-workflow-recipes` |
| AC7 | `restructure verify --against HEAD` reports no moved logic |
| AC8 | `./test -p tddy-core -p tddy-github -p tddy-workflow-recipes` passes at baseline test counts |

## Out of scope

- **The PR-stack data model** (`pr_stack/`, `orchestrate_pr_stack/{git_ops,assess,pr_insight,actions}`)
  — `#carve` 9/9, which consumes both of this node's outputs.
- `changeset.rs` — `#carve` 4/9.
- Any behaviour change to git invocation, retry, or error text.

## Why this needs `#carve` 1/9 but not 3/9

`orchestrate_pr_stack/github.rs` is **nested**, so `source_crate_of` refuses it today; and both moves
leave `pub use` facades, which is the shape that trips the cycle refusal. Both are 1/9's fixes.

The three GitHub files are a **DAG, not a cycle** — `github_pr → github_rest_common ←
orchestrate_pr_stack::github` — so a **leaf-first ordering sidesteps the cluster defect entirely**:
each rewrite of `crate::github_rest_common` → `tddy_github::github_rest_common` is correct the moment
it is made. The `worktree.rs` move is a single module after Phase A. Neither needs 3/9's cluster
support.
