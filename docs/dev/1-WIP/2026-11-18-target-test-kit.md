# Changeset: Target-session tests share one stand-in kit, and a live test proves the Coder path on a real deployment

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Type**: Refactor (tests) + production test
**Complexity**: 4. Spans four crates (`tddy-testing-commons`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-session-driver`), test-only moves across 11 files, and adds a production test against a real deployment.
**Stack**: `#coder-start` 9/10 — branch `feature/coder-start/target-test-kit`, base `feature/coder-start/start-visibility`

## Initial Discovery

[`2026-11-18-target-test-kit-initial-discovery.md`](2026-11-18-target-test-kit-initial-discovery.md)

## Prerequisites

- ✅ RESOLVED HERE: [`2026-11-16-remote-shell-test-fixtures-are-duplicated.md`](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md)
- ✅ RESOLVED HERE: [`2026-11-16-coder-hosts-never-run-against-a-real-deployment.md`](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)

## Affected Packages

- **`tddy-testing-commons`**: a new `remote_target` module (behind its existing dev-only use).
- **`tddy-agent-launch`, `tddy-session-lifecycle`**:
  - the `tests/support/` module each crate already has for its typed helpers;
  - 11 test files' fixtures swapped for imports.
- **`tddy-session-driver`**:
  - a dev-dep on `tddy-testing-commons`;
  - its acceptance file moved onto the kit;
  - a new `tests/coder_live_production.rs`.
- **`docs/dev/guides/testing.md`**: list the production test.

## Scope

- [ ] Kit module and per-crate support modules
- [ ] Every copy replaced; suites unchanged by name and count
- [ ] Live production test, run once against `mantasi/remote-dev`, with its output recorded in the PR
- [ ] Stand-ins corrected for any fact the live run contradicts
- [ ] Scoped gates

## Responsibility

The shared target-session test fixtures, and the one test that runs the Coder path for real.

## Boundaries

- No production-code change, except a stand-in fact the live run proves wrong. That would be
  recorded as a finding and put to the developer before it is changed.
- Test assertions are unchanged.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/coder-start/coder-transport` | streamed Coder commands over `--stdio`; new stand-in bodies | the live hook-follow check; kit absorbs its stand-ins | change the transport |
| `feature/coder-start/target-agent-start` | target-resolved agents, mint, pre-trust | the live Claude/Cursor starts need them | change the launch |
| `feature/coder-start/start-visibility` | the start's remote-call log | the live test prints it on failure | change the ledger |

## Draft PR contract

Commit 2 is the recorded green baseline of the four suites (names and counts), plus the live test as
a failing production test. The live test is red until nodes 5 and 7 are green beneath it.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no. It consolidates the copies that nodes 5–8 add, and its live test
exercises their behaviour.
**Concurrent with:** none
**Blocks:** the sweep

## Decisions & Trade-offs

- **Test fixtures may be written by hand** (developer, 2026-11-18). The engine is tried first; if
  it refuses (integration test files are crates of their own), the kit is hand-written and the
  copies deleted, assertions unchanged. Production code stays engine-only.
- **Two concerns in one node** (developer, 2026-11-18): consolidation and the live test.

## TODO

- [x] Record initial discovery
- [x] Create PRD
- [x] Create changeset
- [ ] Draft PR contract: baseline + live test (commit 2)
- [ ] USER REVIEW: acceptance tests (6e)
- [ ] TDD Green
- [ ] Validate changes
- [ ] Wrap
