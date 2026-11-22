# Changeset: An orchestrator's session files reach the sessions it starts on other hosts

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — one answer field plus joining an existing cross-host route; two-daemon acceptance tests over the in-memory peer world
**PR**: [#996](https://github.com/uppin/tddy-coder/pull/996)
**Stack**: `#orchestra` 10/11 — branch `feature/orchestra/session-files-cross-host`, base `feature/orchestra/orchestrator-prompt`

## Initial Discovery

[2026-11-22-session-files-cross-host-initial-discovery.md](./2026-11-22-session-files-cross-host-initial-discovery.md)

## Affected Packages

- **tddy-tool-engine**: `attachment_for(caller, daemon_instance_id, name)` — the attachment names the holding daemon
- **tddy-daemon-rpc**: `WriteSessionFile` / `ListSessionFiles` served for a caller held elsewhere (`exec_tool/project_home_remote.rs`), answered by its owner

## Related Feature Documentation

- PRD: [PRD-2026-11-22-session-files-cross-host.md](../../ft/daemon/1-WIP/PRD-2026-11-22-session-files-cross-host.md)

## Summary

`WriteSessionFile`'s attachment carries the orchestrator's `daemon_instance_id`; a session-file call that reaches a daemon not holding the caller is confirmed at the caller's owner and served there.

## Responsibility

- The holding daemon in the session-file attachment.
- The session-file tools on the caller-held-elsewhere route.

## Boundaries

- Does not change how an attachment from another host is materialized at start (existing `ReadHostDocument` path).
- Does not forward the base-freshness tools (deferred by the developer, 2026-11-22).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n3` session-files (merged) | `SessionFileStore`, `WriteSessionFile`/`ListSessionFiles`, `attachment_for`, `exec_tool/session_files.rs` | extends `attachment_for`; routes the tools for a remote caller | change the store, names, sizes or the worker refusal |

## Draft PR contract

The first push of this PR: `attachment_for(caller, daemon_instance_id, name)` with its body `TODO(session-files-cross-host)`, plus the failing tests below.

## Green wave

**Wave:** 1 (its only dependency, n3, is merged)
**Greenable independently:** yes
**Concurrent with:** none (n9 is green; the sweep node n11 follows)
**Blocks:** n11 sweep

## Prerequisites

### ✅ RESOLVED HERE — [2026-11-21-session-file-tools-are-not-forwarded-to-a-peer-host.md](../todo/2026-11-21-session-file-tools-are-not-forwarded-to-a-peer-host.md)

The developer chose to continue it at the stack's TODO gate (2026-11-22).

## Scope

- [x] The attachment names the holding daemon
- [x] Session-file tools served for a caller held elsewhere
- [ ] Feature doc `docs/ft/coder/session-attachments.md` § Session files an orchestrator writes updated through the wrap

## Technical Changes

### State A (Current)

`attachment_for` (`packages/tddy-tool-engine/src/session_file_tools.rs:99`) answers no `daemon_instance_id`; `serve_project_home_for_caller_held_elsewhere` (`packages/tddy-daemon-rpc/src/exec_tool/project_home_remote.rs:60`) routes project-home, git and stack tools only.

### State B (Target)

The attachment works from any host; a session-file call lands in the caller's own session wherever it is sent.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/session_files_cross_host_acceptance.rs` (two in-memory daemons, `support::{a_host, InProcessPeers, HOST_A, HOST_B}`):
- [x] `the_written_files_attachment_names_the_orchestrators_daemon`
- [x] `a_session_file_call_sent_to_another_host_lands_in_the_orchestrators_own_session`
- [x] `a_worker_sessions_session_file_call_is_refused_from_another_host` — guard, green before and after: host-b already refuses a session it does not hold; the new route must keep refusing a worker after confirming at its owner

Unit — `packages/tddy-tool-engine/src/session_file_tools.rs`:
- [x] `a_session_files_attachment_names_the_daemon_that_holds_it`
- [x] `the_attachment_parses_as_a_start_session_attachment` (existing)

Contract surface: `attachment_for(caller, holding_daemon, name)` and `SessionFilePort::holding_daemon()`, both `TODO(session-files-cross-host)`.

## Decisions & Trade-offs

- Continued at the stack's TODO gate (developer, 2026-11-22); the base-freshness tools' cross-host gap stays deferred.
- The n3 test `a_written_session_file_answers_with_the_attachment_that_hands_it_to_a_child` (`orchestrator_session_files_acceptance.rs`) pinned the old attachment shape; its expected attachment gained `daemon_instance_id` (the orchestrator's red phase missed it; edited by the coordinator under the developer's pre-approval).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-session-files-cross-host-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (pre-approved: "finish the remaining ones without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [x] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
