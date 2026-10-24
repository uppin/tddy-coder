# crap: ensure_project_available_from_facilitator

**Location:** `packages/tddy-projects/src/project_provision.rs:150` — `ensure_project_available_from_facilitator`
**Category:** crap
**Detected:** 2026-10-24 by `tddy-tools analyze coverage` + `report` on `packages/tddy-projects` (45 tests across 9 files, join rate 66.2%), measured on a detached checkout of `4348f160a`
**Metrics:** **CRAP 90** · complexity **9** · **never entered by this crate's tests** · rank **2/50** in this crate · 46 lines · nesting depth 2
**Restructure:** no. It needs tests, not a split
**Status:** Open, **unclaimed**. Pre-existing: complexity is unchanged since merge-base `c1df8e142`

## Measurement history

| Run | CRAP | Complexity | Covered | Lines | Note |
|---|---|---|---|---|---|
| 2026-10-24 | 90 | 9 | no | 46 | first detection. `fix-main-agent-harness-se-e79c8de0` touches one line, the `home_agent: None` → `home_session_id: None` field rename in the `ProjectData` literal at line 190. No branch added; complexity is syntactic and identical at `c1df8e142` |

## What the tool found

The facilitator twin of `ensure_project_available_locally`. Cases 1–2 (registered here: return it, or
re-clone it from its stored `git_url`) are copied line for line from the local version. Case 3 clones
`facilitating_remote_url` into `repos_base_dir/<project_id>` and registers the row. This crate's tests
cover `ensure_project_available_locally` (CRAP 10 = complexity 10, entered). None of them enters this
function.

**Not measured:** coverage from other crates. The only caller is
`tddy-agent-launch/src/svc_ensure_project_available_for_start.rs:56`, and the
clone acceptance suites there may run it. The per-crate capture cannot see that.

## Why it matters here

This is the path an owning daemon takes the first time it sees a project (PRD AC37). If the twin's
cases 1–2 drift from the local version, the drift shows up only on a facilitator-provisioned start.

## What would close it

This is ordinary work. Add a unit test for each of the three cases, using the injected `cloner` the
existing `ensure_project_available_locally` tests already use (`mod tests` in the same file). Cases
1–2 are duplicated, so a small shared `registered_working_copy(projects_dir, project_id, &cloner)`
helper for both functions would leave case 3 as the only new code to test.

## Verified by hand

2026-10-24: I read the function at `4348f160a`. It has 2 `if`s and several `?`-propagated calls with
`map_err`/`ok_or_else` closures. I did not re-derive the tool's exact 9. In this PR's
diff the function's only line is the field rename. The duplication of cases 1–2 with
`ensure_project_available_locally` (lines 77–86 vs 160–169) is exact.
