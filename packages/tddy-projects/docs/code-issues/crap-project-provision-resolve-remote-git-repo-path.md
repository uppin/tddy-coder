# crap: resolve_remote_git_repo_path

**Location:** `packages/tddy-projects/src/project_provision.rs:23` — `resolve_remote_git_repo_path`
**Category:** crap
**Detected:** 2026-10-24 by `tddy-tools analyze coverage` + `report` on `packages/tddy-projects` (45 tests across 9 files, join rate 66.2%), measured on a detached checkout of `4348f160a`
**Metrics:** **CRAP 182** · complexity **13** · **never entered by this crate's tests** · rank **1/50** in this crate · 25 lines · nesting depth 4
**Restructure:** no. It needs tests, not a split
**Status:** Open, **unclaimed**. Pre-existing: the function has been the same since before merge-base `c1df8e142`

## Measurement history

| Run | CRAP | Complexity | Covered | Lines | Note |
|---|---|---|---|---|---|
| 2026-10-24 | 182 | 13 | no | 25 | first detection (`#project-home` main-from-New-session-form branch, `fix-main-agent-harness-se-e79c8de0`). That branch does not touch this function |

## What the tool found

The function probes the environment in a fixed order. First `CARGO_BIN_EXE_tddy-remote-git-repo`. Next a
`tddy-remote-git-repo` next to `current_exe()`, with a `deps/` parent stripped. Then a plain sibling of
`current_exe()`. Last, the bare name for a `PATH` lookup. The 9 test targets in this crate never call
it. CRAP treats coverage as a boolean, so the score is `cx² + cx`.

**Not measured:** coverage from other crates. Its callers are
`tddy-agent-launch` (`svc_ensure_project_available_for_start.rs`, `clone_ssh_env.rs`) and
`tddy-session-agents` (`session_agent_clone.rs`). Their clone suites may run it. The per-crate
capture cannot see that.

## Why it matters here

This function decides which binary the facilitator clone runs as `GIT_SSH_COMMAND`. When it picks the
wrong one, the result is a clone failure inside a session start, far from where the mistake was made.
The second and third probes overlap: once the `deps/` strip is a no-op, both check the same
`parent/tddy-remote-git-repo`. Nothing pins the precedence between them.

## What would close it

This is ordinary work, not `/code-restructuring`. Add unit tests for each probe: env var set, env var
empty, the `deps/` parent, a plain sibling, and the `PATH` fallback. If the probes take the exe path and
an env lookup as parameters, the tests do not have to change process-global state. Remove the
duplicate sibling probe at the same time.

## Verified by hand

2026-10-24: I read the function at `4348f160a`. It has 8 `if`/`if let` branches and two closures
(`map(|p| …)`, `and_then(|n| …)`). I did not re-derive exactly how the tool reaches 13 from these. The
score is about what it says it is: a branchy probe that no test enters. The diff `c1df8e142..4348f160a` does not touch these
lines; it only renames `home_agent` to `home_session_id` in two struct literals further down the
file. Not verified: whether another crate's tests enter it (see above).
