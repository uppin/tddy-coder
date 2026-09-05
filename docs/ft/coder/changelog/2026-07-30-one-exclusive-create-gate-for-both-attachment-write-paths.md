# 2026-07-30 — One exclusive-create gate for both attachment write paths

- The attachment store's two entry points — `copy_attachment_into_session` (local file) and `write_attachment_bytes` (bytes already in memory, e.g. a `HostDocumentRef` fetched from a peer) — now share one `create_new(true)` gate, so an existing attachment is refused with `FAILED_PRECONDITION` rather than truncated and a symlink planted in `artifacts/attachments/` is refused rather than followed; previously only the local-file path was hardened.
- A bad `SessionAttachment.basename` is refused with *"attachment basename must be a single path segment"* instead of a message about `upload_id` / `file_name`, fields the attachment API does not have.
- See [session-attachments.md § store](../session-attachments.md#tddy-daemon--store-session_attachments).
