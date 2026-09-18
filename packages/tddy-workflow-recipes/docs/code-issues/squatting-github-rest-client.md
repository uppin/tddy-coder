# squatting: a GitHub REST client in a workflow-recipes crate

**Location:** `packages/tddy-workflow-recipes/src/github_pr.rs`, `src/github_rest_common.rs`, `src/orchestrate_pr_stack/github.rs`
**Category:** squatting
**Detected:** 2026-09-15 by structural audit
**Metrics:** **2,124 lines** · 0 of them name `tddy_core`, the recipe machinery or `pr_stack` · consumed across a crate boundary by `tddy-tools`
**Restructure:** required — move all three to the existing `tddy-github`
**Status:** Open — claimed by #492, in flight
**Claimed by:** #492 — `#carve` 6/10 `git-plumbing` · draft · `feature/carve/git-plumbing`
**Lands after:** #488, #489, #490, #498, #491

## Measurement history

| Run | Lines | Files | Note |
|---|---|---|---|
| 2026-09-15 | 2,124 | 3 | first detection |

## What the tool found

Production-only symbol census, comments stripped:

| File | Lines | Names |
|---|---:|---|
| `github_rest_common.rs` | 338 | **nothing** — a pure leaf |
| `github_pr.rs` | 494 | `crate::github_rest_common` only |
| `orchestrate_pr_stack/github.rs` | 1,292 | `crate::github_rest_common::github_token_from_env` only |

Meanwhile `tddy-github` is 1,340 lines of OAuth, session tokens and a token store with **no PR
surface at all**, and `tddy-workflow-recipes` does not depend on it. `github_pr` is already consumed
**outside** this crate, by `tddy-tools/src/server.rs`.

## Why it matters here

A GitHub REST client in a recipes crate means anything wanting to open a PR depends on every workflow
recipe. That it ever lived here is an accident of where it was first written.

## What would close it

Move all three to `tddy-github`, **leaf-first**: `github_rest_common`, then `github_pr`, then the
nested `orchestrate_pr_stack/github.rs`. Facades at the old paths so `tddy-tools/src/server.rs` is
not edited.

`tddy-github` depends only on `tddy-rpc` and `tddy-service`, so **no cycle is possible**. And the
three files form a **DAG, not a cycle**, so leaf-first ordering makes each rewrite correct the moment
it is made — no multi-module cluster support needed.

## If you are about to change this code

#492 moves the files behind facades: no public path changes, no behaviour changes.

Coordinate if you are **adding a REST call** — it belongs in `tddy-github` after #492, and adding it
here means #492 moves it too.

## Verified by hand

2026-09-15: confirmed each file's `crate::` census and read `tddy-github/src/lib.rs` (six modules,
all OAuth/tokens) and its manifest (two workspace deps) to rule out a cycle.
