# 2026-11-15 — Rename, soft-delete and restore a project

**Type:** Feature

`src/project/`: `fan_out.rs`, `update.rs`, `soft_delete.rs`, `project_sessions.rs`, `lifecycle_listing.rs`, `recreate.rs`; `ListProjects` reconciles and filters deleted projects; the project-home tools refuse a deleted project's caller. `add_project_to_host_at_project_coordinate` measured 120 → 126 lines (the restore branch is one call into `recreate.rs`). See [architecture.md § Module layout](../architecture.md#module-layout).

Cross-package record: [docs/dev/changesets/2026-11-15-project-update-and-soft-delete.md](../../../../docs/dev/changesets/2026-11-15-project-update-and-soft-delete.md).
