# 2026-11-18 — The progress sink carries a start's remote calls and its cut worktree

**Type:** Feature

`AttachmentProgressSink::remote_call(StartRemoteCall)` and `worktree_ready(branch, worktree)` send
the start's `remote_call` / `worktree_ready` events; a closed receiver is ignored, as for every other
event. See [host-documents-and-attachments.md](../host-documents-and-attachments.md). Full record: [`docs/dev/changesets/2026-11-18-start-visibility.md`](../../../../docs/dev/changesets/2026-11-18-start-visibility.md).
