# Daemon product area changelog

## 2026-03-21 — Project concept

- **Projects**: Named `git_url` + `main_repo_path` per user; `~/.tddy/projects/projects.yaml`.
- **Config**: `repos_base_path` (default `repos` under user home).
- **ConnectionService**: `ListProjects`, `CreateProject`; `StartSession` uses `project_id`; `SessionEntry` includes `project_id`.
- **Clone**: On create, clone into `{repos_base}/{name}/` unless path exists (then adopt).
- **Spawn**: `tddy-coder` receives `--project-id`; `.session.yaml` stores `project_id`.
- **PRD reference:** PRD-2026-03-21-project-concept.md (wrapped into [project-concept.md](project-concept.md)).
