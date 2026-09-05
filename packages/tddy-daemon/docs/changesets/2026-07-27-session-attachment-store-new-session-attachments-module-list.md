# 2026-07-27 — session-attachment-store: new `session_attachments` module (`list_session_attachments`, `copy_attachment_into_session`

**Type:** Feature

segment + containment guards, exclusive create-new write with partial-file cleanup); `session_context_docs` lists manifest docs then attachments (`ContextDocKind`, `size_bytes`, recipe-independent attachments); enrichment maps `kind`/`size_bytes` onto `SessionEntry.context_docs`. Docs [connection-service.md § ListSessions workflow fields](../connection-service.md#listsessions-workflow-fields). Feature [session-attachments.md](../../../../docs/ft/coder/session-attachments.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
