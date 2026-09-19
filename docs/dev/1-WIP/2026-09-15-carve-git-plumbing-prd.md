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

**2,124 lines of GitHub REST**, none of which touches the recipe machinery or `pr_stack`.
Meanwhile `tddy-github` exists — 1,340 lines of OAuth, session tokens and a token store —
with **no PR surface at all**, and `tddy-workflow-recipes` does not depend on it.

> **Corrected during green.** The "Depends on" column above lists only `crate::`-qualified
> references, and this section originally read "not one of them touches `tddy_core`". That is false:
> `orchestrate_pr_stack/github.rs` names `tddy_core::WorkflowError` in 16 production signatures,
> fully qualified inline, where a `use` / `crate::` grep does not see it. See FR2.

`github_pr` is already consumed **outside** the crate, by `tddy-tools/src/server.rs`.

## What this PR delivers

### FR1 — a `tddy-git` crate

The pure-git portion of `worktree.rs` moves to a new `tddy-git`. `tddy-core` keeps the session-aware
layer — the two `setup_worktree_for_session*` functions, their `setup_worktree_for_session` wrapper,
and `resolve_persisted_worktree_integration_base_for_session` — which call into `tddy-git` for every
git operation.

> **Corrected during green.** This list originally also named `setup_worktree_for_session_over_ssh`,
> and that is incompatible with AC2. The four functions above are **478 production lines on their
> own**, so the floor for a `worktree.rs` that keeps all five is 488 — AC2 asks for under 450, and no
> formatting slack closes a 47-line gap. `setup_worktree_for_session_over_ssh` is the one item on the
> list that never reads a `Changeset`: it is `git clone` + `git worktree add` over SSH, parameterised
> by a session-id **string**. It moves to `tddy-git` with `ssh_exec.rs` (96 lines, `std`-only), and
> `tddy_core::ssh_exec` becomes a facade. Measured result: **428 production lines**. The "~1,200 lines
> leave" estimate in the problem statement was simply wrong.

`tddy-core::worktree` keeps a facade, so **no consumer is edited**.

### FR2 — the GitHub REST client moves to `tddy-github`

All three files move, in **leaf-first order**: `github_rest_common`, then `github_pr`, then
`orchestrate_pr_stack/github.rs` — which lands as `tddy_github::pr_api`, because
`orchestrate_pr_stack` is a workflow-recipes concept with no meaning inside `tddy-github`.
`tddy-workflow-recipes` gains a `tddy-github` dependency and keeps facades at the old paths, so
`tddy-tools/src/server.rs` is not edited.

> **Corrected during green — two premises in the problem statement were wrong.**
>
> 1. **`orchestrate_pr_stack/github.rs` does touch `tddy_core`.** It states `tddy_core::WorkflowError`
>    in **16 production signatures**, including every method of the public `GithubPrApi` trait. The
>    discovery missed this because the references are fully qualified inline, so a `use` / `crate::`
>    grep does not see them. The client cannot move without that edge, and the developer chose to
>    take it (see AC6).
> 2. **The facade edge closes a package cycle.** `tddy-github` already depends on `tddy-service`
>    (`auth_service.rs` uses `tddy_service::proto::auth`), and `tddy-service` depended on
>    `tddy-workflow-recipes`. Adding `tddy-workflow-recipes → tddy-github` for the facades closes
>    `recipes → github → service → recipes`, and `cargo` refuses to build the workspace at all. This
>    is inherent to FR2 — it would have appeared even if only the two leaf files moved. Resolved by
>    demoting `tddy-service`'s `tddy-workflow-recipes` dependency to a **dev-dependency**: its only
>    three uses are `use tddy_workflow_recipes::TddRecipe;` inside `src/integration_tests.rs`, which
>    `src/lib.rs` declares `#[cfg(test)] mod integration_tests;`. No production path changed, and
>    Cargo permits cycles through dev-dependencies.

### FR3 — no behaviour changes

Both are pure relocations: every moved line is byte-identical to its origin, and the only edits to
moved code are five `fn` → `pub fn` widenings in `tddy-git` for helpers the session layer still calls
across the new crate boundary.

> **Corrected during green.** "Mechanical" was meant as "performed by `tddy-tools restructure`". It
> was not, and could not be: `restructure anchors` currently resolves no item in any file, and
> `move_module_to_crate` refuses any move where a file left behind names the moved module — which is
> precisely the facade shape both halves of this node require. Both moves were done by hand with
> `git mv` plus an editor, and identity was proved with `diff` against `HEAD` rather than with
> `restructure verify` (see AC7).

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `tddy-git` exists, depends on no `tddy-*` crate, and holds no function naming `Changeset` |
| AC2 | `tddy-core/src/worktree.rs` is under 450 production lines and holds only the session-aware layer |
| AC3 | Every pre-existing `tddy_core::worktree::…` path resolves — no consumer edited |
| AC4 | `tddy-github` exposes `github_pr`, `github_rest_common` and the PR-state API from `orchestrate_pr_stack/github.rs` (as `pr_api`) |
| AC5 | Every pre-existing `tddy_workflow_recipes::github_pr::…` path resolves — `tddy-tools/src/server.rs` is not edited |
| AC6 | `tddy-github` gains no dependency on **`tddy-workflow-recipes`** — the crate the client left, which now holds facades pointing back here. **Amended during green:** the original also forbade `tddy-core`, on the false premise that the moved files name nothing from it. They name `WorkflowError` 16 times, so the developer ruled the edge in. It closes no cycle — `tddy-core` depends on neither `tddy-github` nor `tddy-service` nor `tddy-workflow-recipes` |
| AC7 | No moved logic. **Amended during green:** `restructure verify` is not the instrument — it compares an `extract_module` plan's output, and no restructure plan was run (see FR3). Identity is proved directly: `git show HEAD:<old> \| diff - <new>` is empty for all four whole-file moves, and a line-multiset diff of `worktree.rs` against `worktree.rs + tddy-git/src/lib.rs` shows only the five `pub fn` widenings and the new doc/facade lines |
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
