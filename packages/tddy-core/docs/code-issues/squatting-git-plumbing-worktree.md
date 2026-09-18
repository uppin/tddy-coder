# squatting: 1,200 lines of general-purpose git plumbing in the god-crate

**Location:** `packages/tddy-core/src/worktree.rs`
**Category:** squatting
**Detected:** 2026-09-15 by structural audit
**Metrics:** **1,606 production lines** · 48 free functions · **2** of them session-aware · 35 dependents
**Restructure:** required — extract the pure-git portion to a new `tddy-git`
**Status:** Open — claimed by #492, in flight
**Claimed by:** #492 — `#carve` 6/10 `git-plumbing` · draft · `feature/carve/git-plumbing`
**Lands after:** #488, #489, #490, #498, #491

## Measurement history

| Run | Production lines | Functions | Session-aware | Note |
|---|---|---|---|---|
| 2026-09-15 | 1,606 | 48 | 2 | first detection |

## What the tool found

Forty-eight free functions wrapping `git`: `create_worktree`, `git_rev_parse`,
`push_new_branch_to_remote`, `list_worktrees`, `local_branch_name`,
`validate_integration_base_ref`, `fetch_origin_master`, `detect_default_remote_name`,
`remote_branch_ref_sha`, `first_free_suffixed_branch_name`, `remove_worktree`,
`list_recent_remote_branches`, and so on.

The file's **only** non-git import is `worktree.rs:11` —
`use crate::changeset::{read_changeset, write_changeset, BranchWorktreeIntent};` — and it is consumed
in exactly **two** functions:

| Range | Function | Consumption lines |
|---|---|---|
| 817–978 | `setup_worktree_for_session_with_integration_base` | 828, 830, 833, 835, 861, 868, 900, 911, 948, 962 |
| 1146–1348 | `setup_worktree_for_session_with_optional_chain_base` | 1178, 1180, 1183, 1185, 1213, 1221, 1260, 1273 |

Nothing else in 1,606 production lines names a `tddy-core` symbol.

## Why it matters here

~1,200 lines of general-purpose git plumbing are locked inside the crate 35 packages depend on. Any
crate that wants to run `git rev-parse` must depend on the session model, the presenter, the workflow
engine and — until `heavy-dependency-sqlx-session-catalog.md` closes — bundled SQLite.

## What would close it

Split at the measured seam: the pure-git functions to a new **`tddy-git`** that depends on no
workspace crate; the two `setup_worktree_for_session*` functions, their wrapper,
`resolve_persisted_worktree_integration_base_for_session` and `setup_worktree_for_session_over_ssh`
stay in `tddy-core` and call into it. Facade at `tddy_core::worktree` so no consumer is edited.

There is **no cycle to break** — `changeset → worktree` does not exist in production code — so this
is a plain extraction. `/code-restructuring` job. Target: `worktree.rs` under 450 production lines.

## If you are about to change this code

#492 moves the plumbing behind a facade: no public path changes, no behaviour changes, and the
externally-located-worktree refusal recorded in `docs/dev/todo/` must survive intact.

Coordinate if you are **adding a git wrapper** (it belongs in `tddy-git`, not here) or **changing
retry or error text** in an existing one (#492's whole claim is that it changes neither).

## Verified by hand

2026-09-15: grepped every production consumption of the file's three non-git imports and confirmed
all 18 hits fall inside the two named ranges. Confirmed that four of these functions —
`detect_default_remote_name`, `worktree_path_for_branch`, `local_branch_name_for_remote`,
`checked_out_branch_name` — are the ones `tddy-workflow-recipes`' `pr_stack` reaches across the crate
boundary, so they must stay publicly reachable through the facade.
