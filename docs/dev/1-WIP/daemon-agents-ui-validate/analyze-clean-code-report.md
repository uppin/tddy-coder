# Clean code analysis: session workflow files feature

**Scope:** `session_workflow_files.rs`, workflow RPC methods in `connection_service.rs`, `SessionFilesPanel.tsx`, `SessionMoreActionsMenu.tsx`, `sessionWorkflowPreview.ts`, and associated tests.

---

## Summary

The daemon side keeps filesystem policy in one module (`session_workflow_files`) and keeps RPC handlers thin, which matches typical gRPC layering in this repo. The web UI separates preview routing (`workflowPreviewKind`) from rendering (`SessionFilesPanel`) and documents security assumptions for Markdown. Main gaps are **stale module-level documentation** in the RPC integration test file, **repeated test setup** in that file, and a **large inline Markdown renderer** in the panel component that could be split or tested in isolation. Client preview rules use **extensions** while the server uses an **explicit basename allowlist**—intentionally aligned for current filenames but worth watching if new basenames are added.

---

## Strengths

- **Clear naming:** `list_allowlisted_workflow_basenames`, `read_allowlisted_workflow_file_utf8`, `validate_workflow_basename`, and `workflowPreviewKind` read as intent-revealing names.
- **Security-conscious server design:** Canonical session root, traversal checks, allowlist-only basenames, and skipping symlinks that escape the session tree in listing are documented inline.
- **Thin RPC handlers:** `list_session_workflow_files` / `read_session_workflow_file` delegate to `session_workflow_files` after the same auth/session path resolution pattern as neighboring RPCs—consistent with `connection_service.rs` style.
- **Unit tests in Rust:** `session_workflow_files.rs` includes focused tests for allowlist behavior and UTF-8 reads.
- **Web layering:** Preview kind is a small pure function with a dedicated Bun test; Cypress covers acceptance for Markdown safety (no `<script>` DOM) and YAML presentation.
- **Accessibility hooks:** `aria-label`, `role="menu"`, `menuitem`, and `data-testid` usage are consistent with testable UI patterns elsewhere.

---

## Issues

| Area | Issue |
|------|--------|
| Documentation | `session_workflow_files_rpc.rs` module comment still says handlers are “not fully implemented yet (Red)”, which is misleading if the feature is implemented. |
| Naming | `SessionFilesPanel` is accurate for “files + preview,” but “Panel” alone may be confused with a floating panel; acceptable if product naming is fixed. |
| Complexity | `renderSimpleMarkdown` in `SessionFilesPanel.tsx` is a non-trivial state machine (~60 lines) embedded in the same file as the container component. |
| Key generation | `key()` uses a monotonic counter (`el`) for block keys; stable for a given render but unusual versus content-derived keys—fine for static preview, slightly opaque. |
| Menu wiring | `SessionMoreActionsMenu` documents that “Show files” is wired elsewhere; the button only closes the menu—no callback prop yet. Intentional stub, but easy to forget when integrating. |
| Preview coverage | `sessionWorkflowPreview.test.ts` only covers `changeset.yaml` and `PRD.md`; `.session.yaml` and extension edge cases (e.g. `.yml`) are untested at unit level though Cypress uses `changeset.yaml`. |

---

## Duplication

1. **RPC integration tests (`session_workflow_files_rpc.rs`):** Each test repeats: spawn `true` for a PID, tempfile sessions base, `unified_session_dir_path`, `create_dir_all`, `write_session_yaml`, and `test_service` construction. A small helper (e.g. `fn fixture_session(...) -> (ConnectionServiceImpl, PathBuf, &str)`) would reduce noise without hiding behavior.

2. **Auth/session resolution in RPC handlers:** The block from `user_resolver` → `os_user_for_github` → `sessions_base_for_user` → `validate_session_id_segment` → `unified_session_dir_path` duplicates other session-scoped RPCs in the same file. That is **consistent repository-wide duplication** (not introduced only for this feature); extracting a shared `resolve_session_dir(&req)` would be a cross-cutting refactor, not specific to workflow files.

3. **Allowlist vs. client heuristics:** The server enumerates fixed basenames; the client classifies by `.md` / `.yaml` / `.yml`. No duplicated list strings on the client (good). If a new allowlisted file used an unexpected extension, preview routing might drift—document or add a single shared contract test if that becomes a risk.

---

## SOLID / single responsibility

- **`session_workflow_files.rs`:** Single responsibility—filesystem policy and I/O for allowlisted workflow artifacts. No RPC or auth concerns.

- **`connection_service` workflow methods:** Orchestration only (auth, paths, mapping to proto). Fits **Open/Closed** at the service boundary: new behavior belongs in `session_workflow_files` or proto, not scattered.

- **`SessionFilesPanel`:** Mixes **list selection state**, **Markdown rendering**, and **YAML/plain layout** in one component. Acceptable for a small feature; `renderSimpleMarkdown` could be its own module/component to satisfy **SRP** more strictly.

- **`sessionWorkflowPreview.ts`:** Pure classification—excellent **SRP** and easy to test.

- **`SessionMoreActionsMenu`:** UI shell for overflow actions—**SRP** is fine; integration responsibility stays with the parent until a callback is added (**Dependency Inversion** at the call site).

---

## Documentation

- **Rust:** Module and function docs in `session_workflow_files.rs` explain allowlist ordering, symlink/canonical behavior, and UTF-8 expectations—strong.

- **TS/React:** JSDoc on `workflowPreviewKind` and `SessionMoreActionsMenu` clarifies alignment with server allowlist and wiring notes.

- **Gap:** Stale “Red” phase note in `session_workflow_files_rpc.rs` should be updated or removed to avoid confusing future readers.

- **Cypress:** Comments label acceptance criteria (sanitized Markdown, YAML region, menu test id)—helpful for reviewers.

---

## Refactoring suggestions

1. **Fix or remove** the outdated “Red” / “not fully implemented” line in `packages/tddy-daemon/tests/session_workflow_files_rpc.rs`.

2. **Extract** `renderSimpleMarkdown` to `SessionFilesPanel.simpleMarkdown.tsx` or `simpleMarkdownPreview.tsx` (or a `renderSimpleMarkdown` export from a colocated file) and optionally add a shallow unit test—keeps the panel file focused on layout and selection.

3. **Deduplicate** RPC test setup with a shared fixture helper in `session_workflow_files_rpc.rs` (parameters: session_id, extra file writes closure).

4. **Optional:** Add `workflowPreviewKind(".session.yaml")` and `workflowPreviewKind("TODO.md")` cases to `sessionWorkflowPreview.test.ts` for parity with allowlisted names.

5. **When wiring “Show files”:** Add an optional `onShowFiles?: () => void` to `SessionMoreActionsMenu` so the parent owns behavior without forking the component.

6. **Cross-cutting (only if team agrees):** A private method or small type on `ConnectionServiceImpl` for “authenticated session directory from token + session_id” would shrink duplication across many RPCs—not required for this feature alone.

---

*Generated by analyze-clean-code subagent.*
