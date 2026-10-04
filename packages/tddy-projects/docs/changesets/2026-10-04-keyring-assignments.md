# 2026-10-04 — Project account assignments

**Type:** Feature

`ProjectData.accounts` (provider and account id pairs, serde default and skip-if-empty),
`project_storage::set_project_accounts` (replace the whole list, one account per provider), the
`SetProjectAccounts` method on `ProjectHandler`, and `ProjectEntry.accounts` on every project
response. `tddy-projects` gains no dependency on `tddy-credentials`. See
[project-service.md § Account assignments](../project-service.md#account-assignments).
