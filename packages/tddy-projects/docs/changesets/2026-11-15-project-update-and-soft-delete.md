# 2026-11-15 — Rename, soft-delete and restore a project

**Type:** Feature

`ProjectLifecycle` flattened onto `ProjectData` (`deleted_at`, `deleted_by`, `restored_at`), the rename and lifecycle writes, `ProjectLifecycle::newest`, `adopt_project_lifecycle`, `find_deleted_project_matching`, `refuse_if_deleted` / `refuse_if_project_deleted`, `validate_project_name`; `ProjectHandler` gains the three RPCs; provisioning refuses a locally deleted row. See [project-service.md § Soft delete](../project-service.md#soft-delete).

Cross-package record: [docs/dev/changesets/2026-11-15-project-update-and-soft-delete.md](../../../../docs/dev/changesets/2026-11-15-project-update-and-soft-delete.md).
