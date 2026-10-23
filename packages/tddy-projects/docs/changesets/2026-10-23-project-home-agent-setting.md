# 2026-10-23 — Home agent on the project row

**Type:** Feature

`ProjectData.home_agent: Option<String>` (serde default, not written when unset);
`project_storage::set_project_home_agent(dir, id, Option<&str>)` sets or clears it and errors on an
unknown project; `ProjectHandler::set_project_home_agent`. The catalog check is the caller's. See
[the documentation](../project-service.md#home-agent).
