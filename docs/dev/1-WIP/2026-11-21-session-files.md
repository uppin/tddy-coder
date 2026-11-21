# Changeset: An orchestrator writes session files and attaches them to the sessions it starts

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 2 — one write tool into an existing store layout, one new attachment source on an existing start path
**Stack**: `#orchestra` 3/9 — branch `feature/orchestra/session-files`, base `feature/orchestra/child-back-channel`

## Initial Discovery

[2026-11-21-session-files-initial-discovery.md](./2026-11-21-session-files-initial-discovery.md)

## Affected Packages

- **tddy-session-files**: writer for an orchestrator's own session files (`artifacts/files/<name>`), name validation
- **tddy-tool-engine**: `WriteSessionFile` / `ListSessionFiles` tools; `StartSession.attachments` accepts a session-file reference
- **tddy-daemon-rpc**: role guard admits the two tools for orchestrator roles

## Related Feature Documentation

- PRD: [PRD-2026-11-21-session-files.md](../../ft/daemon/1-WIP/PRD-2026-11-21-session-files.md)

## Summary

An orchestrator can write files into its **own** session directory with `WriteSessionFile` (a brief, a plan, a spec) and list them with `ListSessionFiles`. `StartSession` takes such a file as an attachment by name; the daemon copies it into the child's attachments the same way an uploaded file is.

## Responsibility

- `WriteSessionFile`, `ListSessionFiles`, their schema and admission.
- The `session_file` attachment source on `StartSession`.

## Boundaries

- No writes outside the session's own directory (n6 handles granted writes).
- No web UI for session files.

## Dependencies

None inside `#orchestra`. It builds on its base, `#target-parity` 7/7 (#975), and on what is on master.

## Draft PR contract

The first push of this PR, before its implementation: `SessionFileStore::{write, list}`, the two tool definitions, `AttachmentSource::SessionFile`; failing tests below.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes
**Concurrent with:** n1, n2, n4, n5
**Blocks:** n9

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7 · n8 · n9):

    n1 path-reach → n6 access-elicitation, n8 orchestrator-lsp
    n6 access-elicitation → n8 orchestrator-lsp
    n2 child-back-channel → n7 orchestrator-delivery
    n1 … n8 → n9 orchestrator-prompt

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7 · 3 = n8 · 4 = n9 (run `1 1 1 1 1 2 2 3 4`).
Inside wave 1, blockers lead: n1 (3 transitive dependents: n6, n8, n9), n2 (2: n7, n9), then n3, n4, n5
(1 each: n9) in foundational order. Inside wave 2: n6 (2: n8, n9) before n7 (1: n9). n3–n5 and n7 sit
in the line only because `gh stack` needs a line; none depends on its neighbour.

## Prerequisites

### ⚠ DURING — [complexity-start-session-args-schema-schema-offering.md](../../../packages/tddy-tool-engine/docs/code-issues/complexity-start-session-args-schema-schema-offering.md)

The attachment source adds one schema branch; it goes in its own helper so `schema_offering` does not grow.

### Unrelated — [2026-08-01-session-attach-ui-follow-ups.md](../todo/2026-08-01-session-attach-ui-follow-ups.md)

Browser attach UI; not touched.

## Scope

- [ ] `WriteSessionFile` / `ListSessionFiles` for orchestrator roles
- [ ] `session_file` attachment source on `StartSession`
- [ ] Feature doc `docs/ft/coder/session-attachments.md` updated through the wrap

## Technical Changes

### State A (Current)

Attachments: `packages/tddy-session-files/src/session_attachments.rs`; `StartSession.attachments` (`start_session_args.rs:66,75-106`) by reference only; no agent-side writer.

### State B (Target)

An orchestrator writes files into its session and attaches them to children.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/orchestrator_session_files_acceptance.rs`:
- [ ] `an_orchestrator_writes_a_session_file_and_lists_it`
- [ ] `a_session_file_name_with_a_path_separator_is_refused`
- [ ] `a_started_session_receives_the_orchestrators_file_as_an_attachment`
- [ ] `a_worker_session_is_refused_write_session_file`

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- Files live in the orchestrator's own session directory and are copied into the child on start, so a child never reads another session's directory.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-session-files-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
