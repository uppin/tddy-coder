# crap: list_project_branches_at_project_coordinate

**Location:** `packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs:521` — `ProjectRpcHandler::list_project_branches_at_project_coordinate`
**Category:** crap
**Detected:** 2026-10-24 by `tddy-tools analyze coverage` + `report` on `packages/tddy-daemon-rpc` (287 tests across 85 files, join rate 66.8%), measured on a detached checkout of `4348f160a`
**Metrics:** **CRAP 110** · complexity **10** · **never entered by this crate's tests** · rank **2/50** in this crate · 65 lines (fn line to closing brace)
**Restructure:** no. It needs tests
**Status:** Open, **unclaimed**. Pre-existing

## Measurement history

| Run | CRAP | Complexity | Covered | Lines | Note |
|---|---|---|---|---|---|
| 2026-10-24 | 110 | 10 | no | 65 | first detection. **Not touched** by `fix-main-agent-harness-se-e79c8de0` (`c1df8e142..4348f160a`): 65 lines at both |

## What the tool found

`ListProjectBranches` authenticates, validates `project_id`, finds the project, refuses a missing
`main_repo_path`, resolves the effective remote, and lists up to 50 recent remote branches on a
blocking worker. No test in this crate calls it. `git grep ListProjectBranchesRequest` in tests finds
only the `TestDaemon` stub in `src/test_util.rs`.

**Not measured:** `tddy-daemon` suites and other crates. Web callers (`ProjectsScreen`, `ProjectCard`,
`SessionMainPane`, `PrStackScreen`) are exercised only by Cypress component specs, which mock the RPC.

## Why it matters here

The web's branch lists (Projects screen, session pane, PR stack) depend on the `default_remote` this
function returns.
A regression in the remote resolution (`effective_remote_name_for_project`, which is unit-tested in
`tddy-projects`) wired through here would show up only in the browser.

## What would close it

This is ordinary work. Add an acceptance test with a local bare repo carrying two remote branches
that asserts the branch list and `default_remote`, plus the `project_id`-empty, not-found and
missing-path refusals.

## Verified by hand

2026-10-24: I read the function at `4348f160a`. It has 2 `if` refusals, about 6 `?`/`ok_or_else`
exits and a `map_err` closure inside the blocking closure, consistent with complexity 10. It is
unchanged by this branch.
