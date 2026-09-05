# 2026-07-26 — start-session-attachments: `src/gen/connection_pb.ts` regenerated (`bunx buf generate ../tddy-service/proto`, +453 lines) for `StartSessionRequest.attachments`, `SessionAttachment`/`StagedAttachmentRef`/`HostDocumentRef`/`StagedAttachmentEntry`, `HostDocumentScope`, and the three staging RPCs

**Type:** Feature

**codegen only, no hand-written web code**: no picker, no host-document browser, nothing calls the new RPCs (the daemon returns `UNIMPLEMENTED`). `docs/terminal-file-upload.md` gains a "Reuse: start-session attachments" note recording that `chunkFile()`/`UPLOAD_CHUNK_SIZE` and the per-file chunk loop carry over unchanged (only the request builder differs) and that a staged batch is usable only by a session started on the host it was uploaded to. Verified by `bun run build`; `src/buildId.ts` deliberately not committed. Docs [terminal-file-upload.md](../terminal-file-upload.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
