# Start-session attach UI — implementation

Product spec: [session-attachments.md](../../../docs/ft/coder/session-attachments.md).
Host contract: [connection-service.md § Start-session attachments](../../tddy-daemon/docs/connection-service.md#start-session-attachments).

Lets an operator attach documents to a session **at creation time**, from `CreateSessionPane` — which
is shared by the drawer's **New session** flow and the PR-stack **Start session** dialog, so both get
it. Two sources: local files (uploaded) and documents already on a connected host (referenced, no
upload).

| Module | Role |
|--------|------|
| `hooks/useSessionAttachments.ts` | Owns the attachment state, the cap, rename/remove, `stageAttachments()` and the streamed-start consumer |
| `hooks/useStagedAttachmentUpload.ts` | Per-file chunk loop against `UploadStagedAttachmentChunk`, one `staging_id` per submit |
| `components/sessions/attachments/AttachmentDropZone.tsx` | Drop target + drag-over overlay + the two pick affordances |
| `components/sessions/attachments/SessionAttachmentList.tsx` | One row per attachment: source, editable basename, size, progress, remove |
| `components/sessions/attachments/HostDocumentPicker.tsx` | Browses a host by `HostDocumentScope`, yields a `HostDocumentRef` |
| `components/sessions/attachments/pendingAttachment.ts` | The row model shared by the above |
| `lib/attachmentBasenames.ts` | Basename validation + collision detection, mirroring the host's rules |
| `lib/attachmentBytes.ts` | Binary-unit formatting (`8 MiB`), which the decimal `formatBytes` cannot produce |

Reused rather than rebuilt: `lib/fileUploadChunks.ts` (`chunkFile`, `UPLOAD_CHUNK_SIZE`) and
`lib/randomId.ts` from [terminal-file-upload.md](terminal-file-upload.md); `WorktreeFileTree` +
`worktreeFilesApi` from `components/session/` for the two tree-browsing picker scopes.

## Invariants worth not breaking

**1. Nothing uploads until submit, and a retry uploads nothing again.** Picking a file only adds a
row. `stageAttachments()` runs from `submitCreation`, and its result is memoized per row — keyed on
**both** the `File` object identity and the staging host. That second key is not incidental: without
it, a resubmit after the operator switched daemons would reuse a `staging_id` on a host that never
received the bytes. The memo matters because `submitCreation` is re-entered by
`resolveBranchConflict`; before it existed, choosing "rename" at a branch conflict re-uploaded every
attachment under a fresh `staging_id` (~18 min for a 50 MB file at the ~47 KB/s this transport
measures) and orphaned the first batch until a host restart.

**2. `streamStartSession` only when there are attachments.** With none, the pane calls unary
`StartSession` exactly as before — the no-attachment path is untouched, and a spec guards it by making
the streaming stub throw. Always streaming would have changed the request path for every existing
create-session spec for no benefit.

**3. Consume the stream with a `cancelled` flag, never `AbortController`.**
`packages/tddy-livekit-web/src/transport.ts`'s `handleServerStreaming` accepts `_signal` and never
reads it — no abort frame is published and the queue is not closed. The in-memory test router *does*
honour the signal, so an `AbortController`-based implementation passes its spec while the real stream
keeps running in production. `components/sessions/useSessionActivity.ts` is the reference idiom.

**4. A stream error is a failed creation, not an interrupted progress bar.** The terminal `result`
event is what calls `onCreated`; an error terminates the stream and must surface in the form's error
strip with Create re-enabled. The host also reports an invalid token as a stream error rather than at
call time on the local path, so treating errors as merely cosmetic would silently swallow an auth
failure.

**5. Basenames collide case-insensitively.** `duplicateBasenames` folds with `toLowerCase` (not
`toLocaleLowerCase`, so the result does not depend on the operator's locale). This is stricter than a
Linux host, which would happily store `Spec.md` and `spec.md` side by side — but the host writes with
`create_new(true)`, so on macOS's case-insensitive APFS the second attachment returns
`FAILED_PRECONDITION` and fails the **entire** `StartSession`. Refusing a pathological pair of names
is a smaller loss than a session creation that breaks on one supported OS. This is the second place
the client is deliberately stricter than the host; whitespace-only names are the first.

**6. Each picker scope has its own `relative_path` shape, and getting it wrong fails silently.** The
host answers a bad path with `NOT_FOUND` late, at materialization:

| Scope | Listed via | `relative_path` | Ref carries |
|-------|-----------|-----------------|-------------|
| `SESSION_ARTIFACT` | `SessionEntry.context_docs` | basename for a `MANIFEST` doc; **`attachments/<basename>`** for an `ATTACHMENT` doc | `session_id` |
| `SESSION_UPLOAD` | `ListSessionUploads` | `<upload_id>/<file_name>` | `session_id` |
| `SESSION_WORKTREE` | `ListWorktreeDirectory` on `SessionEntry.repo_path` | path within the worktree | `session_id` |
| `PROJECT_REPO` | `ListWorktreeDirectory` on `ProjectEntry.main_repo_path` | path within the repo | `project_id` |

Two traps in that table. `SESSION_WORKTREE` must list under the **session's own** `project_id`, not
the project being created in, because `resolve_listed_worktree` checks
`worktree_path_is_listed(main_repo_of(project_id), worktree_path)`. And a context doc with
`exists == false` is never offered — the recipe declared it but never wrote it, so picking it would
only earn a `NOT_FOUND`. `PROJECT_REPO` works by listing the project's **primary** worktree, which
`ListWorktreeDirectory` accepts; that is pinned host-side by
`worktree_files_rpc.rs::list_worktree_directory_lists_the_projects_primary_worktree` rather than
assumed.

**7. The cap is a `min` across two hosts, so the message must not name one.** Both the staging host
(which stores the bytes) and the session host (which re-checks while fetching) bound an attachment, so
the form refuses against the smaller of the two advertised `max_attachment_bytes` values. When neither
advertises one, the client enforces nothing and the host still does. The cap applies to all four
picker scopes as well as local files — every scope's listing carries a size, which is why
`worktreeFilesApi.listDir` surfaces `WorktreeDirEntry.size_bytes` instead of discarding it.

**8. The staging host and the session host are different things.** Uploads go to the daemon the
form's `client` is connected to; the session runs on `effectiveDaemonInstanceId`, which the Host
selector can point elsewhere and which peer mode freezes. Each staged ref is stamped with the
**staging** host, and the session host fetches across when they differ.
`HostDocumentPicker`'s `browsedDaemonInstanceId` is the staging host for the same reason — none of the
listing RPCs carry a `daemon_instance_id`, so listing one host while stamping another would produce
refs to documents that host does not have.

## Tests

Cypress `CreateSessionAttachmentsAcceptance` (12), `CreateSessionHostDocumentPicker` (8),
`CreateSessionAttachmentProgress` (4). Units `attachmentBasenames.test.ts`,
`attachmentBytes.test.ts`, and the cap-propagation case in `participantRole.test.ts`.

Mid-stream progress is asserted on `data-attachment-percent` with the stub generator held at a gate,
so the value is exact rather than whatever the race settled on — the technique
`TerminalFileUploadProgressFooter.cy.tsx` established. Note that `anInMemoryRpcBackend` records
**unary** calls only, so a streaming request has to be captured in a closure; `callsTo` will not see
it.

**Not covered anywhere:** the browser→daemon leg. Every Cypress spec stubs the RPCs, so real chunk
uploads over the LiveKit data channel, a real streamed `StartSession` and a real cross-host fetch have
never run from a browser. A manual two-daemon `./web-dev` verification is outstanding — see
[TODO.md](../../../docs/dev/TODO.md).
