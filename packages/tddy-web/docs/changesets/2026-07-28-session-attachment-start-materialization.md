# 2026-07-28 — **session-attachment-start-materialization

**Type:** Feature

`connection_pb.ts` regenerated to carry `ReadHostDocument`** — the generated TS bindings now include the `ReadHostDocument` RPC + `ReadHostDocumentRequest`/`Response` so the upcoming Start-Session attachment picker and host-document browser can call it; no web behavior shipped in this slice (the UI is a separate changeset). Feature [session-attachments.md § Start-session materialization](../../../../docs/ft/coder/session-attachments.md#start-session-materialization). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)
