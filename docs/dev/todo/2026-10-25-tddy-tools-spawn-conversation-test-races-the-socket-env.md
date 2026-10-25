# 2026-10-25 — `spawn_conversation_errors_when_tddy_socket_is_unset` races an env-setting test

**Category:** Pre-existing test defect (not made worse by `#coagents` 7/8, #726)
**Source:** `packages/tddy-tools/src/server.rs` (`tests::spawn_conversation_errors_when_tddy_socket_is_unset`)

The test builds a `PermissionServer` and expects no relay socket. It is not `#[serial]`, while
`github_create_pull_request_returns_the_session_hosts_refusal_to_the_agent` (and its siblings) are
`#[serial]` and set `TDDY_TOOLS_TEST_ALLOW_SOCKET=1` and `TDDY_SOCKET` for their duration. Under plain
`cargo test -p tddy-tools --lib` (parallel threads) the unserialised test can read those variables
and fail. `./test` runs with `--test-threads=1`, so the repo's gate does not see it.

Observed on #726: fails once in a parallel run, passes alone and 120/120 on a rerun. #726 adds no
env-setting lib test; it reads `TDDY_CONVERSATION_ID` in `PermissionServer::new`, which no lib test
sets.

## What would close it

Mark the test `#[serial]` (it reads the same process-global variables), or inject the socket path
into `PermissionServer` instead of reading the environment in `new`.
