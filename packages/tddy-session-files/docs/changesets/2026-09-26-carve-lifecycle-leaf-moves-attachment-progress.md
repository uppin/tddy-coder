# 2026-09-26 — Holds the attachment materialisation progress types

**Type:** Refactor

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

`attachment_progress` (`AttachmentProgressSink`, `AttachmentProgressReporter`,
`AttachmentMaterialization`, `cleanup_materialized_attachments`, `attachment_size_bytes`) moved here
from `tddy-session-lifecycle`'s `connection_service`, which brings it into scope with a
`pub(crate) use`; its items are `pub`. No new dependencies. Production lines 4,600 → 4,716; tests
160 passed, unchanged. The materialisation itself stays in lifecycle: it reads host state. See
[host-documents-and-attachments.md](../host-documents-and-attachments.md#materialisation-progress).
