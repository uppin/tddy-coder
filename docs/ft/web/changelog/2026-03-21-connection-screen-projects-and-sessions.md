# 2026-03-21 — Connection screen: projects and sessions

- **ConnectionScreen**: Lists projects (`ListProjects`), inline create project (`CreateProject` with optional path-under-home), accordion sections per project with sessions filtered by `projectId`, **Start New Session** per project (`StartSession` with `project_id`). Orphan sessions section when `project_id` is unknown to the list.
- **Removed**: Manual repository path field; work is always scoped to a project.
