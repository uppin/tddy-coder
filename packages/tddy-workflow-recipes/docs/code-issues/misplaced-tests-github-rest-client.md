# misplaced-tests: the GitHub REST client's acceptance suites

**Location:** `packages/tddy-workflow-recipes/tests/` — `github_pr_acceptance.rs`, `github_qualified_head_acceptance.rs`, `pr_status_view_acceptance.rs`
**Category:** misplaced-tests
**Detected:** 2026-09-22 by `/pr-wrap` on #492
**Metrics:** 3 test binaries · 283 lines · every `use tddy_workflow_recipes::…` in them names a facade over `tddy_github`
**Restructure:** required — `move_test_binary_to_crate` (or `git mv`), `/code-restructuring` territory
**Status:** Open

## Measurement history

| Run | Binaries | Lines | Note |
|---|---|---|---|
| 2026-09-22 | 3 | 283 | first detection — created by #492 moving the code they test |

## What the tool found

#492 (`#carve` 6/11) moved the GitHub REST client to `tddy-github` and left one-line facades behind.
These three suites import **only** those facades:

| Suite | Imports |
|---|---|
| `github_pr_acceptance.rs` | `tddy_workflow_recipes::github_pr::{…}`, `tddy_workflow_recipes::github_rest_common::github_token_from_env` |
| `github_qualified_head_acceptance.rs` | `tddy_workflow_recipes::orchestrate_pr_stack::github::qualified_head` |
| `pr_status_view_acceptance.rs` | `tddy_workflow_recipes::orchestrate_pr_stack::github::{pr_state_from_github, PrState}` |

So they test `tddy-github` code while being compiled against, and counted in, `tddy-workflow-recipes`.

The other nine suites that import `orchestrate_pr_stack::github` also import recipe code
(`pr_stack`, `orchestrate_pr_stack` actions, `tests/common`), so they are **correctly placed** —
they test the recipes' use of the client, not the client.

## Why it matters here

A failure in any of the three reports against the wrong crate, and a change to `tddy-github` does not
run them under `./test -p tddy-github`. They were left where they were because #492's boundary was
"no consumer edited", and moving a test binary is exactly the kind of edit it ruled out.

## What would close it

Move all three to `packages/tddy-github/tests/`, re-pointing each `tddy_workflow_recipes::github_pr`
→ `tddy_github::github_pr`, `…::github_rest_common` → `tddy_github::github_rest_common`, and
`…::orchestrate_pr_stack::github` → `tddy_github::pr_api`. `tddy-github` already has `serial_test`,
`serde_json` and `rstest` for them. The recipes-side test count drops by the moved tests and
`tddy-github`'s rises by the same number — check both.

## Verified by hand

2026-09-22: collected every top-level `use tddy_workflow_recipes::` line in each of the crate's test
binaries and kept those whose every import names one of the three facade paths.
