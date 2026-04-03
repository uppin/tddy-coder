# Changeset: Session workflow files preview (RPC + web)

**Status:** 🚧 In Progress  
**Branch:** `feature/session-workflow-files-preview`  
**Created:** 2026-04-03

## Summary

Adds allowlisted listing and UTF-8 reads of workflow files under a session directory (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`), exposed via `ConnectionService` RPCs, with web UI to browse and preview content (Markdown/YAML). Server-side path validation prevents traversal and escapes outside the session directory.

## Affected packages

- `packages/tddy-service` — `connection.proto` RPCs + messages
- `packages/tddy-daemon` — `session_workflow_files`, `ConnectionService` handlers, docs
- `packages/tddy-integration-tests` — formatting-only edits in green session contract test
- `packages/tddy-web` — `SessionFilesPanel`, menu wiring, Cypress CT, unit tests for preview kind
- `docs/ft/daemon`, `docs/ft/web` — changelog / web-terminal notes
- `docs/dev/1-WIP/daemon-agents-ui-validate/*` — validation reports (supporting)

## Implementation Progress

**Last Synced with Code**: 2026-04-03 (via @validate-changes)

**Core features**:

- [x] Proto: `ListSessionWorkflowFiles`, `ReadSessionWorkflowFile` — ✅ Complete (`connection.proto`)
- [x] Daemon: allowlist + canonical path checks — ✅ Complete (`session_workflow_files.rs`)
- [x] Daemon: RPC auth + `validate_session_id_segment` + unified session dir — ✅ Complete (`connection_service.rs`)
- [x] Web: files panel + preview — ✅ Complete (`SessionFilesPanel.tsx`, `sessionWorkflowPreview.ts`)
- [x] Web: Cypress component coverage — ✅ Complete (`SessionWorkflowFiles.cy.tsx`)
- [x] Daemon integration-style tests — ✅ Complete (`session_workflow_files_rpc.rs`)
- [x] Unit tests (Rust + TS) — ✅ Complete (`session_workflow_files` mod tests, `sessionWorkflowPreview.test.ts`)

**Docs / product notes**:

- [x] Daemon + service `changesets.md` entries — ✅ Complete
- [x] `connection-service.md` RPC documentation — ✅ Complete
- [x] Feature changelogs and `web-terminal.md` updates — ✅ Complete

**Testing**:

- [x] Rust: `cargo build -p tddy-daemon -p tddy-service -p tddy-integration-tests` — ✅ Passed (2026-04-03)
- [x] TS unit: `sessionWorkflowPreview.test.ts` — ✅ Passed
- [x] Rust: `cargo test -p tddy-daemon session_workflow_files` + `--test session_workflow_files_rpc` — ✅ Passed (3 integration + 2 unit)
- [ ] Full workspace `./test` / `./verify` — 🔲 Not run in this validation pass (run before merge)

## Acceptance criteria

- [x] Only allowlisted basenames; traversal and out-of-session symlinks rejected
- [x] Authenticated RPCs consistent with other session-scoped methods
- [x] Web preview avoids raw HTML injection for Markdown path (structured React nodes / escaped text)

### Change Validation (@validate-changes)

**Last Run**: 2026-04-03  
**Status**: ⚠️ Warnings  
**Risk Level**: 🟢 Low

**Changeset sync**:

- 🆕 Changeset created; branch had no prior `🚧 In Progress` entry in `docs/dev/1-WIP/`
- PRD context: `docs/ft/daemon/1-WIP/PRD-2026-03-19-tddy-daemon.md` (daemon scope; workflow file preview aligns with session/orchestration UX)

**Documentation validation** (context present — full `@feature-doc` / `@dev-doc` sweep skipped):

- Feature docs: changelogs and `web-terminal.md` updated on branch
- Dev docs: `packages/tddy-daemon/docs/connection-service.md` updated

**Analysis summary**:

- Packages built: 3 Rust crates (tddy-daemon, tddy-service, tddy-integration-tests) — all success; warnings: `Git tree is dirty` only
- Build warnings in changed code: none observed
- Files analyzed: 21 files in `master...HEAD` (+ dirty `buildId.ts`)
- Critical issues: 0
- Warnings: working tree hygiene (see below)

**Risk assessment**:

- Build validation: Low
- Test infrastructure: Low (no test-only production branches added)
- Production code: Low (allowlist + canonicalize; session token on RPCs)
- Security: Low (path allowlist; XSS mitigated in preview component)
- Code quality: Low–Medium (`renderSimpleMarkdown` is long; acceptable for scoped mini-renderer)

**Working tree notes** (not in `master...HEAD`):

- `packages/tddy-web/src/buildId.ts` modified — expected after `prebuild` / `gen-build-id.mjs`; avoid committing unless releasing a built bundle
- `.tddy-red-cargo-test.log` untracked — remove or add to `.gitignore`; do not commit as product artifact

## Refactoring Needed

### From @validate-changes (optional)

- [ ] Consider splitting `renderSimpleMarkdown` in `SessionFilesPanel.tsx` if it grows further (currently ~60 lines)
- [ ] Run full `./verify` before merge and attach evidence from `.verify-result.txt`
