# 2026-11-15 — Rename, soft-delete and restore a project

**Type:** Feature

`project.proto`: `UpdateProject`, `DeleteProject`, `RestoreProject` and `UnconfirmedHost`; `ProjectEntry.deleted_at` / `deleted_by` / `restored_at`; `ListProjectsRequest.include_deleted`. The three RPCs joined the closed residual-methods list in `unbundle_service_split`.

Cross-package record: [docs/dev/changesets/2026-11-15-project-update-and-soft-delete.md](../../../../docs/dev/changesets/2026-11-15-project-update-and-soft-delete.md).
