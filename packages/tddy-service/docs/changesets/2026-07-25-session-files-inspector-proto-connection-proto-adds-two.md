# 2026-07-25 — session-files-inspector: `proto/connection.proto` adds two unary RPCs on `ConnectionService`

**Type:** Feature

`ListSessionUploads(session_token, session_id) → repeated SessionUploadEntry{upload_id, file_name, host_path, size_bytes, uploaded_at_ms}` (newest first) and `DeleteSessionUpload(session_token, session_id, upload_id, file_name) → {}` — backing the web Inspector Files tab (regenerated Rust + `tddy-web/src/gen/connection_pb.ts`). Feature [session-files-inspector.md](../../../../docs/ft/web/session-files-inspector.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-service)
