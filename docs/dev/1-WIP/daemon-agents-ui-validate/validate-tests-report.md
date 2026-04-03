# Validate tests report — session workflow files

**Worktree:** `session-workflow-files-preview`  
**Date:** 2026-04-03  
**Scope:** `tddy-daemon` session workflow file RPCs + `tddy-web` session workflow preview UI.

---

## Summary

| Suite | Result | Count |
|--------|--------|--------|
| `cargo test -p tddy-daemon` | **PASS** | 62 tests (0 failed) |
| `bun test` — `sessionWorkflowPreview.test.ts` | **PASS** | 2 tests (0 failed) |
| Cypress CT — `SessionWorkflowFiles.cy.tsx` | **PASS** | 3 tests (0 failed) |

**Primary command exit status:** `./dev cargo test -p tddy-daemon` → **0** (success).

All requested automated checks passed. Coverage is strong for daemon allowlisting and RPC contracts; web coverage is thin for unit tests and absent for end-to-end integration of the new UI with `ConnectionScreen` and live RPCs.

---

## Commands run

1. **Rust (daemon package)**  
   ```bash
   cd /var/tddy/Code/tddy-coder/.worktrees/session-workflow-files-preview
   ./dev cargo test -p tddy-daemon
   ```

2. **Bun unit tests (workflow preview kind)**  
   ```bash
   ./dev bash -c 'cd packages/tddy-web && bun test src/components/session/sessionWorkflowPreview.test.ts'
   ```

3. **Cypress component (optional)**  
   ```bash
   ./dev bash -c 'cd packages/tddy-web && bun run cypress:component -- --spec cypress/component/SessionWorkflowFiles.cy.tsx'
   ```

---

## Results

### `cargo test -p tddy-daemon`

| Binary / target | Tests | Outcome |
|-----------------|-------|---------|
| `src/lib.rs` (unit) | 35 | ok |
| `src/main.rs` | 0 | ok |
| `tests/acceptance_daemon.rs` | 8 | ok |
| `tests/delete_session.rs` | 2 | ok |
| `tests/grpc_spawn_contract.rs` | 1 | ok |
| `tests/list_agents_allowlist_acceptance.rs` | 4 | ok |
| `tests/list_sessions_enriched.rs` | 1 | ok |
| `tests/multi_host_acceptance.rs` | 5 | ok |
| **`tests/session_workflow_files_rpc.rs`** | **3** | **ok** |
| `tests/signal_session.rs` | 3 | ok |
| Doc-tests | 0 | ok |

**Session-workflow–related unit tests** (in `session_workflow_files` via `lib.rs`):  
`list_allowlisted_workflow_basenames_includes_allowlisted_fixture_files`, `read_allowlisted_workflow_file_utf8_returns_exact_bytes_for_changeset_yaml`.

**Integration (`session_workflow_files_rpc.rs`):**  
`list_session_workflow_files_returns_allowlisted_basenames`, `read_session_workflow_file_returns_utf8_content_for_yaml`, `read_session_workflow_file_rejects_path_outside_session_dir`.

### Bun — `sessionWorkflowPreview.test.ts`

- `workflowPreviewKind` classifies `changeset.yaml` → yaml  
- `workflowPreviewKind` classifies `PRD.md` → markdown  

### Cypress — `SessionWorkflowFiles.cy.tsx`

- `SessionFilesPanel` — markdown preview when MD selected  
- `SessionFilesPanel` — YAML preview when YAML selected  
- `SessionMoreActionsMenu` — includes **Show files** (`data-testid="session-more-actions-show-files"`)

Non-fatal noise during Cypress: DBus warning, Vite CJS deprecation notice, port 5173 busy (fallback), `resize: can't open terminal /dev/tty` — **did not fail the run**.

---

## Failures

**None.** All commands exited with status **0**.

---

## Coverage gaps

1. **ConnectionScreen integration**  
   `SessionFilesPanel` and `SessionMoreActionsMenu` are **not imported** in `ConnectionScreen.tsx` (or elsewhere outside `cypress/component/SessionWorkflowFiles.cy.tsx`). There is no component test or e2e test that opens a connected session, uses **Show files**, and asserts list/read behavior against a mocked or real daemon.

2. **Stale generation / RPC refresh**  
   No automated test covers invalidating or refetching file contents when the session directory changes on disk (e.g. after a workflow step writes `TODO.md`). Any client-side “generation” or cache semantics would need explicit tests once wired.

3. **Bun unit test surface**  
   `sessionWorkflowPreview.test.ts` only exercises `workflowPreviewKind` for two basenames. It does not cover `.session.yaml`, `TODO.md`, unknown extensions, or edge cases aligned with the server allowlist (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`).

4. **Daemon edge cases**  
   RPC tests cover happy paths and directory traversal rejection. Additional cases worth considering later: empty list when no allowlisted files exist, non-UTF8 file handling (if specified), symlink edge cases beyond what unit tests already assert.

5. **E2E**  
   No Cypress e2e (or Playwright) scenario for “user sees workflow files in app shell” — only isolated CT with static props.

---

## Recommendations

1. **Wire and test** `SessionMoreActionsMenu` + `SessionFilesPanel` from `ConnectionScreen` (or the session row container), then add **ConnectionScreen**-level Cypress coverage with RPC stubs/fixtures for `ListSessionWorkflowFiles` / `ReadSessionWorkflowFile` (or equivalent client calls).

2. **Extend** `sessionWorkflowPreview.test.ts` with one test per allowlisted basename and one “unknown/plain” case if the product defines behavior.

3. **Add** a focused integration or CT test for **refresh after file change** once the UI holds fetched content (document expected behavior in the same PR).

4. **Optional:** run the full `bun run cypress:component` suite in CI before merge if not already, to catch regressions in shared harnesses.

---

*Report generated by validate-tests subagent.*
