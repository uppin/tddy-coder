# Changeset: Start-session attachment materialization

**PRD (amendment)**: `docs/ft/coder/amendments/session-attachments-start-materialization.md`
**Amends**: `docs/ft/coder/session-attachments.md` (the store)
**Wire contract**: `packages/tddy-daemon/docs/connection-service.md` § *Start-session attachments*
**Branch**: `feature/session-attach-docs/attach-start-1`

## Checklist

- [x] Create/update PRD documentation (amendment)
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit tests
- [x] `tddy-daemon`: staging store (`session_attachment_staging`) — root, chunked writer, list, delete
- [x] `tddy-daemon`: `ReadHostDocument` resolver (scope roots, `relative_path` validation, capped binary read)
- [x] `connection.proto`: add `ReadHostDocument` RPC + `ReadHostDocumentRequest`/`Response`
- [x] Regenerate `connection_pb.ts`
- [x] `tddy-daemon`: staging RPC handlers (`upload_staged_attachment_chunk` / `list_staged_attachments` / `delete_staged_attachment`) with multi-host forwarding
- [x] `tddy-daemon`: `ReadHostDocument` handler with multi-host forwarding
- [x] `livekit_peer_discovery`: `forward_upload_staged_attachment_chunk_via_livekit` / `forward_list_staged_attachments_via_livekit` / `forward_delete_staged_attachment_via_livekit` / `forward_read_host_document_via_livekit`
- [x] `tddy-daemon`: `materialize_session_attachments` helper + wiring into all `StartSession` session-type paths
- [x] `tddy-daemon`: pre-create `session_dir` for the tool (tddy-coder) branch so attachments land before the child spawns
- [x] Doc fix: `connection-service.md` `{session_dir}/attachments/` → `{session_dir}/artifacts/attachments/`
- [x] Changelog + package/cross-package changeset index entries (wrap phase)

## Implementation status

✅ Complete. Green-phase implementation landed; amendment folded into `docs/ft/coder/session-attachments.md`; `connection-service.md` § *Start-session attachments* corrected and marked implemented. 23 new tests (unit + acceptance, including the LiveKit multi-host forwarding suite) pass after a test-harness fix (the `two_daemons()` helper now owns its `TempDir` guards so the project/session data survives for the forwarding assertions).

## State A (before this changeset)

- Store: `packages/tddy-daemon/src/session_attachments.rs` — `copy_attachment_into_session`, `list_session_attachments` (landed by the store changeset).
- Staging RPCs: `connection_service.rs:7887` — three `async fn` stubs returning `Status::unimplemented`, marked `TODO(start-session-attachments)`.
- `StartSession` (`connection_service.rs:4228`): ignores `req.attachments`; dispatches by `session_type` to workspace / claude-cli / sandboxed claude-cli / cursor-cli / tool branches. The non-tool branches generate `session_id = Uuid::now_v7()` and the handler creates `session_dir` (`workspace_session.rs:50`, etc.); the tool branch lets the spawned child create `session_dir` (`tddy-coder/src/run.rs:1344`).
- Multi-host forwarding: `livekit_peer_discovery::forward_to_peer` + thin `forward_*_via_livekit` wrappers for `StartSession` / `AddProjectToHost` / `SetProjectDefaultBranch`. `classify_peer_route` resolves `PeerRoute::Local` / `Forward { peer_instance_id }`.
- Per-user path resolution: `user_sessions_path::sessions_base_for_user(os_user, Some(&tddy_data_dir))` → `{tddy_data_dir}` (the daemon's data root, already per-OS-user when configured).
- Guards reused everywhere: `session_file_upload::{validate_segment, contained_canonical_dir}` (`pub(crate)`).

## State B (target)

See the amendment. Staging is implemented, `ReadHostDocument` is added, `StartSession` materializes both sources before spawn, staging + fetch RPCs forward across hosts, and the doc path is corrected.

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-daemon/src/session_attachment_staging.rs` | Staging store: `StagedAttachmentFile`, `staging_root_for(os_user, tddy_data_dir)`, `write_staged_chunk`, `list_staged_attachments`, `delete_staged_attachment` |
| `packages/tddy-daemon/src/host_documents.rs` | `ReadHostDocument` resolver: `read_host_document_bytes(os_user, tddy_data_dir, scope, session_id, project_id, relative_path) -> Result<HostDocumentBytes, Status>` (scope root resolution + capped binary read) |
| `packages/tddy-daemon/tests/staging_rpc_acceptance.rs` | Acceptance: staging upload/list/delete + `StartSession` materialization (staged + host-document), multi-host forwarding, cross-host refusal, duplicate-basename rejection |
| `packages/tddy-daemon/src/host_documents.rs` (`mod tests`) | Unit: scope root resolution, `relative_path` validation, cap refusal, binary-safe read |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-service/proto/connection.proto` | Add `rpc ReadHostDocument` + `ReadHostDocumentRequest`/`ReadHostDocumentResponse` messages (binary, capped). The staging/`SessionAttachment`/`HostDocumentRef`/`HostDocumentScope` messages are already on `master`. |
| `packages/tddy-web/src/gen/connection_pb.ts` | Regenerate (`./dev bun run --filter tddy-web generate`) so the web gen carries `ReadHostDocument`. |
| `packages/tddy-daemon/src/lib.rs` | `pub mod session_attachment_staging; pub mod host_documents;` |
| `packages/tddy-daemon/src/connection_service.rs` | Implement the three staging handlers + `read_host_document` (replace the `TODO(start-session-attachments)` stubs); add `materialize_session_attachments` and call it in every `StartSession` session-type path after `session_dir` exists and before spawn; pre-create `session_dir` for the tool branch |
| `packages/tddy-daemon/src/livekit_peer_discovery.rs` | Four thin `forward_*_via_livekit` wrappers for the staging RPCs + `ReadHostDocument` |
| `packages/tddy-daemon/docs/connection-service.md` | § *Start-session attachments*: `{session_dir}/attachments/` → `{session_dir}/artifacts/attachments/`; mark staging + `ReadHostDocument` as implemented |
| `docs/ft/coder/session-attachments.md` | (wrap phase) fold the amendment in: add staging, `ReadHostDocument`, and materialization sections; keep present tense |
| `docs/ft/coder/changelog.md` | Release note (wrap phase) |
| `docs/dev/changesets.md`, `packages/tddy-{daemon,service,web}/docs/changesets.md` | Index entries (wrap phase) |

## Design decisions

### Staging root is per-OS-user under the daemon data dir

`{tddy_data_dir}/staging/{os_user}/{staging_id}/{file_name}`. Per-`os_user` so one mapped user cannot read another's staged batches; the `session_token` → `github_user` → `os_user` resolution already gates every other per-user path. `staging_id` and `file_name` are untrusted basenames validated with `validate_segment`, and the per-batch dir is canonicalize-and-contained under `{tddy_data_dir}/staging/{os_user}/` (the trusted root, holding no untrusted component) — the same guard shape as `session_file_upload`.

### Staging mirrors the terminal upload flow, not the store

`write_staged_chunk` is `session_file_upload::write_upload_chunk` with `sessions_base/{session_id}` replaced by `{staging_root}/{staging_id}`: same ordered-append, same `create(true).append(true)`, same `last` semantics, same final-path return. This keeps `tddy-web`'s `lib/fileUploadChunks.ts` reusable unchanged (per the wire-contract note) and the per-chunk size inside one LiveKit data packet.

### `ReadHostDocument` is unary, binary, capped, and forwardable

Unary because streaming RPCs return `unimplemented` for `PeerRoute::Forward` (the handoff's constraint). Binary because attachments may be images/PDFs — the UTF-8 readers (`ReadSessionWorkflowFile`, `ReadWorktreeFile`) are the wrong shape. Capped at `MAX_HOST_DOCUMENT_BYTES` (4 MiB, matching gRPC's default max message size) and **refused** over-cap with `INVALID_ARGUMENT` rather than truncated — a truncated attachment is useless, and staging exists for larger docs (chunked, no single-message limit). Forwardable by `daemon_instance_id` like the staging RPCs.

### The owning daemon resolves scope roots under its own `os_user`

`HostDocumentRef` is "a document that already exists on a connected host"; the owning daemon performs the read under **its own** `os_user` mapping. The referencing client's host grants no access — so `ReadHostDocument` re-resolves `session_token` → `os_user` on the owning daemon (whether local or the forwarded peer), exactly as `ListSessions` / `ReadWorktreeFile` do. `relative_path` is POSIX-separated, no `.`/`..`, not absolute, and canonicalize-and-contained under the resolved scope root.

### `SESSION_WORKTREE` reuses the listing gate

For `HOST_DOCUMENT_SCOPE_SESSION_WORKTREE`, `relative_path` must be a path surfaced by `ListWorktreeDirectory` (`.git`-excluded, `.gitignore`-aware) — the same gate `ReadWorktreeFile` uses. This prevents `ReadHostDocument` from becoming a wider read surface than the existing worktree file reader.

### `StartSession` materializes before spawn, for all session types

A single `materialize_session_attachments(os_user, tddy_data_dir, sessions_base, session_id, &req.attachments, common_room_livekit_room)` helper is called in every session-type path after `session_dir` exists and before the spawn. For the non-tool branches this is one new call each (the handler already creates `session_dir`); for the tool branch the daemon pre-creates `session_dir` at `{sessions_base}/sessions/{session_id}/` (using the `session_id` it already passes via `SpawnOptions.new_session_id`) and materializes before `spawn_as_user` / `spawn_worker`. The child's `create_session_dir_with_id` is idempotent on an existing dir, so the pre-created dir is reused. This unifies `session_dir` creation in the daemon for the tool branch (today only the conversation-spawn recipe pre-creates it).

### Cross-host `StagedAttachmentRef` is a request error, not a fetch

A `StagedAttachmentRef` whose `daemon_instance_id` names a host other than the one running the session is `INVALID_ARGUMENT` — never a cross-host fetch, never a silent empty attachment. The wire contract pins this; the materializer enforces it. `HostDocumentRef` is the *only* source that performs a cross-host fetch (via `ReadHostDocument`), because by design the document lives on a host that may differ from the session host.

### Duplicate basenames rejected at the request boundary

The materializer collects `basename`s and rejects duplicates **before** any `copy_attachment_into_session` call — a request error (`INVALID_ARGUMENT`), so the store's `FAILED_PRECONDITION` (which guards a single later copy against an earlier one in the same request) is never the duplicate-basename signal. This keeps the error code semantically distinct: `INVALID_ARGUMENT` = bad request shape; `FAILED_PRECONDITION` = an attachment with that name already exists on disk (e.g. from a prior call — not reachable from a single `StartSession`).

### Failure cleans up partial attachments

If any attachment in the request fails to materialize, `StartSession` removes `artifacts/attachments/` entries written for this request so far, then returns the error. No half-materialized session reaches the agent. (The session_dir itself is left to the handler's own failure path.)

## Acceptance tests

`packages/tddy-daemon/tests/staging_rpc_acceptance.rs` — end-to-end through `ConnectionServiceImpl`:

1. `a_staged_attachment_referenced_by_start_session_lands_under_artifacts_attachments_before_the_agent_runs` — stage a file via `UploadStagedAttachmentChunk`, then `StartSession` with one `SessionAttachment { staged }`; assert the file exists at `{session_dir}/artifacts/attachments/<basename>` and the spawned agent command received the session_dir (or a sentinel) — i.e. materialization preceded spawn.
2. `start_session_refuses_a_staged_attachment_ref_naming_a_foreign_daemon_instance_id` — `StartSession` addressed to the local daemon with a `StagedAttachmentRef.daemon_instance_id` naming a peer returns `INVALID_ARGUMENT`; nothing is written.
3. `start_session_rejects_duplicate_basenames_within_one_request_before_writing_any_attachment` — two `SessionAttachment`s with the same `basename` (different staging_ids) → `INVALID_ARGUMENT`; `artifacts/attachments/` stays empty.
4. `a_host_document_ref_to_a_session_artifact_copies_the_bytes_into_attachments` — a session with an existing `artifacts/PRD.md` referenced via `HostDocumentRef { scope = SESSION_ARTIFACT, relative_path = "PRD.md" }`; after `StartSession`, `artifacts/attachments/PRD.md` exists with the source bytes (renamed, not replacing the manifest doc).
5. `a_host_document_ref_with_a_relative_path_escaping_the_scope_root_is_refused` — `relative_path = "../outside"` → `INVALID_ARGUMENT`; no attachment written.
6. `a_host_document_ref_naming_a_peer_daemon_is_forwarded_via_read_host_document` — two-daemon LiveKit setup (reuse `multi_host_acceptance` harness); `StartSession` on daemon A with a `HostDocumentRef` naming daemon B resolves the doc on B and copies bytes into A's session.
7. `staging_rpcs_addressed_to_a_peer_daemon_forward_and_operate_on_the_peer_staging_root` — `UploadStagedAttachmentChunk` / `ListStagedAttachments` / `DeleteStagedAttachment` with `daemon_instance_id` = peer route to the peer and read/write its staging root.
8. `a_host_document_ref_to_a_file_over_the_cap_is_refused` — a scope root file larger than `MAX_HOST_DOCUMENT_BYTES` → `INVALID_ARGUMENT`.

## Unit tests

`packages/tddy-daemon/src/session_attachment_staging.rs` (`mod tests`):

1. `write_staged_chunk_appends_ordered_chunks_and_returns_the_path_on_the_final_chunk`
2. `write_staged_chunk_rejects_an_unsafe_staging_id_or_file_name_as_invalid_argument`
3. `write_staged_chunk_refuses_to_overwrite_an_existing_file_name_within_a_batch`
4. `list_staged_attachments_returns_one_batch_newest_first_when_staging_id_is_set`
5. `list_staged_attachments_returns_every_batch_for_the_caller_when_staging_id_is_empty`
6. `delete_staged_attachment_removes_one_file_and_rejects_an_unsafe_segment`
7. `a_staging_directory_symlinked_outside_the_staging_root_is_refused` (unix)

`packages/tddy-daemon/src/host_documents.rs` (`mod tests`):

8. `read_host_document_resolves_session_artifact_scope_under_the_callers_os_user`
9. `read_host_document_resolves_session_upload_scope_with_upload_id_slash_file_name`
10. `read_host_document_resolves_project_repo_scope_under_the_projects_main_repo_path`
11. `read_host_document_refuses_a_relative_path_with_dotdot_segments`
12. `read_host_document_refuses_an_absolute_relative_path`
13. `read_host_document_refuses_a_file_over_the_cap_without_truncating`
14. `read_host_document_returns_bytes_verbatim_for_a_binary_file` (non-UTF-8 survives)
15. `read_host_document_refuses_a_worktree_relative_path_not_surfaced_by_the_listing`

`packages/tddy-daemon/src/session_attachments.rs` (existing store) — no new unit tests; the materializer uses `copy_attachment_into_session` unchanged. The materializer's duplicate-basename and cross-host-refusal logic is covered by acceptance tests (it is request-level policy, not store-level).

## Known follow-ups (out of scope here)

- **Staging GC** — consumed-batch cleanup after `StartSession` consumes a batch + TTL for abandoned batches. Tracked in `docs/dev/TODO.md` (source: this changeset).
- **Streaming `ReadHostDocument`** for docs over `MAX_HOST_DOCUMENT_BYTES` — needs a forwardable streaming design (today's streaming RPCs return `unimplemented` for `PeerRoute::Forward`).
- **Web UI** — Start-Session attachment picker + host-document browser in `tddy-web`.

## Validation Results

- **Tests**: 25 new tests pass — `tests/staging_rpc_acceptance.rs` (7: staged→session, cross-host refusal, duplicate-basename, host-document SESSION_ARTIFACT, path-escape refusal, over-cap refusal, **incomplete-staged-upload refusal**), `tests/staging_forwarding_acceptance.rs` (2: two-daemon LiveKit testkit — staging RPC forward + `ReadHostDocument` forward), and unit tests in `session_attachment_staging` (7) / `host_documents` (9, incl. **symlink-escape refusal**). A test-harness fix was required: `two_daemons()` now owns its `TempDir` guards in a struct so the project/session data survives for the forwarding assertions (the previous helper dropped them early, deleting the fixtures mid-test).
- **Lint/format**: `cargo fmt --check` clean; `cargo clippy -p tddy-daemon -p tddy-service --tests -- -D warnings` clean. A pre-existing deprecated-constant reference (`DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF`) in `tests/telegram_session_control_integration.rs` was refreshed to `FALLBACK_DEFAULT_INTEGRATION_BASE_REF` so the test target stays clippy-clean.
- **Code review (bugbot)**: ✅ 4 findings, all fixed before merge:
  - **HIGH — symlink escape in `ReadHostDocument`**: `read_host_document_bytes` now canonicalizes the **full file path** (not just the parent dir) and re-checks containment against the canonical scope root, so a symlinked file inside the scope root that points outside is refused (regression test `read_host_document_refuses_a_symlinked_file_escaping_the_scope_root`).
  - **MEDIUM — materializes incomplete staged uploads**: fixed (see security review below).
  - **MEDIUM — reads entire file before cap check**: `read_host_document_bytes` now checks `std::fs::metadata(...).len()` against `MAX_HOST_DOCUMENT_BYTES` before `std::fs::read`, so an oversized file is refused without loading its full contents into memory (the post-read length check is retained as defense in depth).
  - **MEDIUM — no cap on forwarded host bytes**: `materialize_host_document_attachment` now re-checks `MAX_HOST_DOCUMENT_BYTES` on the session host before writing forwarded `ReadHostDocument` bytes, so a buggy/older peer cannot push an oversized blob into the session's attachments.
- **Security review**: ✅ no critical/high issues. One medium finding **fixed before merge**: `materialize_staged_attachment` now requires the staged file's `.staged-complete` marker (written only on the final chunk) before copying, so an in-progress or aborted chunked upload can no longer be materialized as a truncated attachment (regression test `start_session_refuses_a_staged_attachment_whose_upload_is_not_complete`). All other focus areas (path traversal, cross-tenant isolation, symlink escape in staging, cross-host staged-ref refusal, duplicate basenames, over-cap refusal, partial-materialization cleanup, tool-branch `session_dir` pre-creation, `SESSION_WORKTREE` listing gate, peer forwarding re-auth) validated clean.
- **Production readiness**: no mock code, no `TODO`/`FIXME` left in the new production paths (the old `TODO(start-session-attachments)` stubs were removed when the handlers landed); no test-only branches in production code; no fallbacks added.
