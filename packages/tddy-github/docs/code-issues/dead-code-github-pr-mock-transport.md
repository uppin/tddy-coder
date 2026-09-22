# dead-code: github_pr.rs — MockGithubTransport

**Location:** `packages/tddy-github/src/github_pr.rs` — `MockGithubTransport`, `create_pull_request`, `update_pull_request`
**Category:** dead-code — *production*-dead: referenced only from a test binary
**Detected:** 2026-09-22 by `/pr-wrap` step 3 (`/validate-prod-ready`) on #492
**Metrics:** 1 struct + 2 public functions compiled into production · 0 production callers · 1 test binary caller
**Restructure:** not required — ordinary work, not a move
**Status:** Open
**Moved:** was `packages/tddy-workflow-recipes/src/github_pr.rs` — relocated byte-identical by #492 (`#carve` 6/11)

## Measurement history

| Run | Production callers | Test callers | Note |
|---|---|---|---|
| 2026-09-22 | 0 | 1 (`tddy-workflow-recipes/tests/github_pr_acceptance.rs`) | first detection |

## What the tool found

A test double lives outside any `#[cfg(test)]`, as public API:

```rust
/// Test-only transport that records GitHub REST calls without network I/O.
#[derive(Debug, Default)]
pub struct MockGithubTransport { pub requests: Vec<RecordedHttpRequest> }
```

and `create_pull_request(transport: &mut MockGithubTransport, …)` /
`update_pull_request(transport: &mut MockGithubTransport, …)` take it. The real path is
`create_pull_request_via_rest_api` / `update_pull_request_via_rest_api`, which is what production
calls: `tddy-tools/src/server.rs` imports only those two.

`grep -rln MockGithubTransport packages/` finds the file itself and
`tddy-workflow-recipes/tests/github_pr_acceptance.rs` — nothing else.

## Why it matters here

CLAUDE.md's rule is **no code branches in production that only work in a test environment**. This is
the milder form — not a branch, but a pair of public functions whose only transport is a recorder —
and it is exactly what a new caller could pick up by mistake: `create_pull_request` is the *obvious*
name, and it silently does no network I/O. The doc comment says "test-only"; the compiler does not
enforce it.

## What would close it

Gate the three items `#[cfg(any(test, feature = "test-support"))]` and have the acceptance test
enable the feature — or, better, turn the transport into a trait the `_via_rest_api` functions take,
so tests pass the recorder and production passes `curl`, and the duplicate pair goes away. Either is
an API change to `tddy_github::github_pr`, so it needs its own PR; #492's boundary is relocation only.

## Verified by hand

2026-09-22: read the struct and both functions; confirmed `server.rs:26-29` imports only the
`_via_rest_api` pair; grepped every package for the type name.
