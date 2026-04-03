# Validate Production Readiness Report — Session Workflow Files

## Summary

Server-side **listing and reading** of session workflow artifacts is **appropriately locked down**: a fixed basename allowlist, `session_id` validation via `validate_session_id_segment`, resolution under `unified_session_dir_path`, and **canonical path checks** so symlink-based escapes are skipped or rejected. The web **preview layer** (`SessionFilesPanel`) avoids `dangerouslySetInnerHTML` and uses a minimal Markdown renderer with React text nodes, which is a sound baseline for XSS.

**Not production-complete end-to-end:** `packages/tddy-web/src/gen/connection_pb.ts` is **not regenerated** from `connection.proto` (no `ListSessionWorkflowFiles` / `ReadSessionWorkflowFile` in the Connect client), **`ConnectionScreen` does not call** these RPCs or mount the session files UI, and **`SessionMoreActionsMenu`’s “Show files”** is a stub (closes the menu only). Daemon handlers run **synchronous filesystem work** on the async RPC path, and reads have **no size bound** (memory/DoS consideration). Repo hygiene: **`.tddy-red-cargo-test.log`** remains untracked noise.

---

## Security

| Area | Assessment |
|------|------------|
| **Session directory** | `validate_session_id_segment` rejects empty, long, multi-segment, and non `[a-zA-Z0-9_-]` ids (`tddy-core`); consistent with other session RPCs. |
| **Basename / traversal** | `validate_workflow_basename` rejects `..`, `/`, `\`, and any non-allowlisted name; only `changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`. |
| **Symlinks & escapes** | `canonicalize()` + `starts_with(&session_root)` on both list and read; outside paths log `warn` and skip (list) or `permission_denied` (read). |
| **Sensitive files** | `.env` is not allowlisted; list test asserts it never appears. |
| **AuthZ** | Same pattern as peers: valid `session_token` → GitHub user → mapped OS user → that user’s `sessions_base`; users cannot target another user’s tree without their token. |
| **Web rendering** | Preview does not parse arbitrary HTML; content is shown as text or simple blocks—low XSS risk for workflow files. |

**Residual risks:** Unbounded `read_to_string` for allowlisted files (very large files → memory pressure). `log::info!` in `read_allowlisted_workflow_file_utf8` records **byte length** but labels it as “UTF-8 chars” (misleading, not a security issue).

---

## Error Handling & Logging

- **gRPC mapping:** Invalid basename → `invalid_argument`; missing/inaccessible session dir → `failed_precondition`; missing file after canonicalize → `not_found`; escape → `permission_denied`; read failure → `internal` with error string (appropriate for server-side I/O failure).
- **`log::debug`:** Used for validation failures, canonicalize skips, and path diagnostics—suitable volume for production when default level is `info`.
- **`log::info`:** Emitted on **every successful** list and read in `session_workflow_files.rs` and again in `connection_service.rs` (duplicate-style success lines). Under busy dashboards this may be **noisier than necessary**; consider a single summary line at the RPC layer or downgrade inner module success to `debug`.
- **`log::warn`:** Used when a symlink or layout resolves outside the session tree—appropriate for security-relevant events.
- **`log::error`:** On unexpected `read_to_string` failure—appropriate.

---

## Configuration

- **No new daemon YAML keys** for this feature: allowlist is **compile-time** in `WORKFLOW_FILE_ALLOWLIST`. Operators cannot widen/narrow files without a release—predictable for security, inflexible if product later needs per-org toggles.
- **Proto** defines `ListSessionWorkflowFiles` / `ReadSessionWorkflowFile`; **web generated types are stale** until `buf generate` (or the project’s codegen script) is run and committed.

---

## Performance

- **Sync I/O on async RPC path:** `list_session_workflow_files` and `read_session_workflow_file` are `async fn` but call blocking `std::fs` and `canonicalize` inline. Under load, this can **block the Tokio worker** thread. Production-hardening options: `tokio::task::spawn_blocking` for the filesystem work, or a small dedicated blocking pool—only needed if these endpoints see meaningful concurrency.
- **List:** Up to four allowlisted paths × `exists` / `canonicalize` / `metadata`—bounded small work.
- **Read:** Full file into a `String`; no streaming or cap.

---

## Production Gaps

1. **Web ↔ daemon contract:** `connection_pb.ts` does not include the new RPCs; the browser cannot type-safely call them until codegen is refreshed and wired.
2. **Product integration:** `ConnectionScreen` does not fetch workflow files or render `SessionFilesPanel`; session table UX is unchanged for file preview.
3. **SessionMoreActionsMenu:** “Show files” does not open a panel, navigate, or trigger RPCs—placeholder only.
4. **Operational logging:** Duplicate `info` success logs (module + service) and possible high volume on automated polling if added later without backoff.
5. **Resource limits:** No max bytes for `ReadSessionWorkflowFile` response body.
6. **Test comment drift:** `session_workflow_files_rpc.rs` header still says handlers are “not fully implemented yet (Red)”—stale vs current implementation.
7. **Repository hygiene:** `.tddy-red-cargo-test.log` (and similar) should be gitignored or deleted before merge to avoid accidental commits and reviewer noise.

---

## Recommendations

1. **Ship checklist:** Run **Buf / protoc codegen** for `packages/tddy-web`, commit `connection_pb.ts`, then implement **list + read** in `ConnectionScreen` (or a child component) with loading/error states.
2. **Wire UX:** Connect **`SessionMoreActionsMenu`** “Show files” to state that shows **`SessionFilesPanel`** (drawer/modal) and loads files for the selected `session_id`.
3. **Performance:** Move filesystem work to **`spawn_blocking`** (or document that call volume is low enough to accept blocking).
4. **Safety / SRE:** Add a **max file size** (e.g. 1–4 MiB) for reads; return `invalid_argument` or `resource_exhausted` when exceeded.
5. **Logging:** Deduplicate success **`log::info`** between `session_workflow_files` and `connection_service`; fix the **“UTF-8 chars”** log message to **bytes** or use `content.chars().count()` if character count is intended.
6. **Hygiene:** Add `.tddy-red-*.log` to `.gitignore` if these artifacts are routine, or delete local copies; refresh the **Red** comment in the RPC test file.
7. **Future configuration:** If the allowlist must vary by deployment, move it to config with explicit review—avoid silent broadening of readable paths.

---

*Scope: `session_workflow_files.rs`, `connection_service.rs` workflow RPC handlers, `connection.proto`, `packages/tddy-web/src/components/session/*.tsx`, `sessionWorkflowPreview.ts`, and known evaluation gaps.*
