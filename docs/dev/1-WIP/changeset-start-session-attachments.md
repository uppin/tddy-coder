# Changeset: Start-session attachments

**Branch**: `feature/session-attach-docs/attach-proto`

Attach documents to a session **at start time**, before the session (and its directory) exists. Two
sources: documents picked in the Start-Session form and uploaded from the browser, and documents
that already live on one of the connected hosts (another session's planning docs, a previous
session upload, a checked-in project doc).

This slice is **proto + generated stubs only** — the wire contract is pinned so the web and the
daemon can be built against it independently. **No runtime behavior**: nothing writes or reads a
staging area, and `StartSessionRequest.attachments` is ignored by the daemon's `start_session`.

## Checklist

- [x] `connection.proto`: pre-session staging RPCs (`UploadStagedAttachmentChunk`,
      `ListStagedAttachments`, `DeleteStagedAttachment`) + `StagedAttachmentEntry`
- [x] `connection.proto`: `StartSessionRequest.attachments` (field 29) + `SessionAttachment`,
      `StagedAttachmentRef`, `HostDocumentRef`, `HostDocumentScope`
- [x] Regenerate Rust stubs (build script: prost/tddy-rpc + tonic passes over `connection.proto`)
- [x] Regenerate `packages/tddy-web/src/gen/connection_pb.ts`
- [x] `tddy-daemon`: `unimplemented` trait stubs so both `ConnectionService` impls compile
- [ ] `tddy-daemon`: staging root + chunked writer (per-caller, basename-validated)
- [ ] `tddy-daemon`: materialize `StartSessionRequest.attachments` into
      `{session_dir}/attachments/` before spawn; refuse a staged ref from another host
- [ ] `tddy-daemon`: `HostDocumentRef` scope resolution + cross-host fetch by `daemon_instance_id`
- [ ] `tddy-daemon`: staging GC (consumed batches; TTL for abandoned ones)
- [ ] `tddy-web`: attachment picker in the Start-Session form, reusing the terminal "Attach"
      chunked-upload client (`chunkFile`, `UPLOAD_CHUNK_SIZE`)
- [ ] `tddy-web`: host-document browser for referencing existing docs on connected hosts
- [ ] PRD in `docs/ft/web/`, acceptance tests, package changelogs

## Files modified (this slice)

| File | Change |
|------|--------|
| `packages/tddy-service/proto/connection.proto` | 3 staging RPCs + 7 messages/enums; `StartSessionRequest.attachments = 29` |
| `packages/tddy-web/src/gen/connection_pb.ts` | Regenerated (`bunx buf generate ../tddy-service/proto`) |
| `packages/tddy-daemon/src/connection_service.rs` | `unimplemented` stubs for the 3 new RPCs, with a `TODO(start-session-attachments)` |
| `packages/tddy-daemon/src/connection_tonic_adapter.rs` | Delegation for the 3 new RPCs |

## Design decisions

**Staging mirrors the terminal "Attach" upload, minus the session.** `UploadStagedAttachmentChunk`
is `UploadSessionFileChunk` with `session_id` replaced by `daemon_instance_id` and `upload_id` by
`staging_id`. The client keeps its existing chunking (48 KiB, one unary per chunk, `last` on the
final one, completed entry returned on the last response), so the web reuses
`lib/fileUploadChunks.ts` unchanged and one chunk still fits a single LiveKit data packet.

**A staged batch belongs to one host.** There is no session to route by, so the upload names its
target host explicitly. A `StagedAttachmentRef` naming a different host than the one `StartSession`
runs on is a **request error**, not a cross-host fetch — no silent fallback that would upload to A
and start on B with an empty attachment.

**Host authority is part of every reference.** Both source variants carry `daemon_instance_id`.
For `HostDocumentRef` the read is performed by the owning daemon under **its own** os_user mapping;
the referencing client's host grants it no access.

**`HostDocumentRef` is scope + relative path, never an absolute host path.** A raw path field would
let any caller name any file the daemon's user can read. Instead `HostDocumentScope` selects a root
the owning daemon resolves itself (session artifacts / session uploads / session worktree / project
repo) and `relative_path` is validated against it. Adding a source of documents means adding a
scope, which is the point — each one is reviewed.

**`basename` is separate from the source locator.** The name the agent sees is decided by the
request, so the UI can rename an attachment without touching the stored file. Duplicate basenames
in one request are rejected rather than silently renamed.

## Not done here

`packages/tddy-rust-typescript-tests/gen/connection_pb.ts` is **not** regenerated. That checked-in
copy is ~4000 lines behind the current proto and nothing in that package imports it; regenerating
it would fold unrelated catch-up drift into this changeset. It needs its own doc-only cleanup PR
(or deletion).
