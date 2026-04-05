# Refactoring plan (post-validate)

Consolidated from `validate-tests-report.md`, `validate-prod-ready-report.md`, and `analyze-clean-code-report.md`.

## Priority 1 — Production polish

1. **Logging:** Remove or gate `console.debug` / `console.info` in `sessionSelection.ts` and `SessionTableSelectAllCheckbox` / bulk-delete paths; avoid logging raw `sessionId` in production builds.
2. **Bulk delete failure path:** On RPC error after partial success, call `listSessions` (or prune selection against current `sessions`) so UI matches server; optionally clear only successfully deleted ids.

## Priority 2 — Tests & CI

3. **Workspace tests:** Document or script that full `./verify` builds `tddy-acp-stub` before integration tests; avoid `cargo test -q` alone in CI without that prerequisite.
4. **Coverage gaps:** Add targeted tests for empty table, stale selection ids, and bulk-cancel (confirm false) if not already implied by CT.

## Priority 3 — Structure (optional)

5. **Duplication:** Extract shared session-table row/header/bulk-toolbar fragment for project vs orphan tables if the file grows further.
