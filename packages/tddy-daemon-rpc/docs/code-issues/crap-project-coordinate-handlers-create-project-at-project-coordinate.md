# crap: create_project_at_project_coordinate

**Location:** `packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs:81` — `ProjectRpcHandler::create_project_at_project_coordinate`
**Category:** crap
**Detected:** 2026-10-24 by `tddy-tools analyze coverage` + `report` on `packages/tddy-daemon-rpc` (287 tests across 85 files, join rate 66.8%), measured on a detached checkout of `4348f160a`
**Metrics:** **CRAP 240** · complexity **15** · **never entered by this crate's tests** · rank **1/50** in this crate · 109 lines (fn line to closing brace) · brace depth 6
**Restructure:** no. Tests come first, then optionally `extract_method` for the clone dispatch
**Status:** Open, **unclaimed**. Pre-existing

## Measurement history

| Run | CRAP | Complexity | Covered | Lines | Note |
|---|---|---|---|---|---|
| 2026-10-24 | 240 | 15 | no | 109 | first detection. `fix-main-agent-harness-se-e79c8de0` changes one line, `home_agent: None` → `home_session_id: None` in the `ProjectData` literal (`:170`). No branch added; complexity is syntactic and identical at `c1df8e142`. That branch deleted `tests/set_project_home_agent_acceptance.rs`, but that suite never called `CreateProject`, so the deletion lost this function no coverage |

## What the tool found

`CreateProject` does all of the following, and no test in `tddy-daemon-rpc/tests` reaches it:

- authenticates and maps the OS user;
- validates `name` (empty, `/`, `..`) and `git_url`;
- resolves the destination (user-relative path under home, or `<repos base>/<name>`);
- clones through one of two spawn backends (`Supervisor` / `ForkedWorker`, the latter with or without a spawn client);
- canonicalizes, registers the row, and answers with the entry.

`git grep` finds `CreateProjectRequest` only in `src/test_util.rs`, the `TestDaemon` forwarding stub, and in
`tddy-web` Cypress component specs, which mock the RPC.

**Not measured:** `tddy-daemon` integration suites, which may drive `CreateProject` over the wire.
The per-crate capture cannot see that.

## Why it matters here

This is the one RPC that clones a repository as another OS user. Its name validation (`/`, `..`) is
the only thing that keeps `base.join(name)` inside the repos base, and no test pins it.

## What would close it

This is ordinary work. An acceptance test in `tddy-daemon-rpc/tests` modelled on
`add_project_to_host_acceptance.rs` (which took `add_project_to_host_at_project_coordinate` from
CRAP 702 to 23), against a local bare repo, plus refusal cases for the three validation errors. After
that, an optional `extract_method` for the backend `match` (lines ~123–150) would take the function
under 60 lines.

## Verified by hand

2026-10-24: I read the function at `4348f160a`. It has 4 validation `if`s, the user-relative
`if`/`else`, a 2-arm backend `match` with an inner `if let`, and several `?` early exits, which
matches a complexity in the teens. The diff `c1df8e142..4348f160a` touches only line 170, the field
rename. The deleted `set_project_home_agent_acceptance.rs` at `c1df8e142` has no `CreateProject`
call.
