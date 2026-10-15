# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

`ProjectData.local_gh` (serde default, written only when true); `project_storage::set_project_accounts(dir, id, accounts, local_gh)` writes it with the accounts in one replace and refuses a GitHub assignment together with it (`InvalidArgument`, naming both, row untouched). `ProjectEntry.local_gh` is returned on every project response. See [the documentation](../project-service.md#local-gh).
