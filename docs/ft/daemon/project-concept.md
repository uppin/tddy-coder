# Project concept

**Status:** Current  
**Supersedes (connection UX):** Session selection by raw repo path alone — users now work through **Projects**.

## Summary

A **Project** is a named configuration linking a **git URL** to a **main repository path** on disk (the clone root, not a git worktree). Projects are **per OS user**, stored in `~/.tddy/projects/projects.yaml`. **Sessions** reference a **project id** in `.session.yaml` and in the `ConnectionService` API.

## Data model

| Field | Description |
|--------|----------------|
| `project_id` | UUID, assigned at creation |
| `name` | User-chosen name; also used as the directory name under the repos base |
| `git_url` | Remote URL (e.g. `https://github.com/org/repo.git`) |
| `main_repo_path` | Absolute path to the cloned repository |

## Storage

- **Projects registry:** `~/.tddy/projects/projects.yaml` (list of projects).
- **Clone location:** `{home}/{repos_base_path}/{name}/` where `repos_base_path` comes from daemon config (default: `repos`).

## Daemon configuration

```yaml
# Optional; default is "repos" under each user's home directory
repos_base_path: "repos"
```

## Create project behavior

1. Resolve destination: `{repos_base}/{name}/`.
2. **If that path already exists** (checked as the target OS user): **no clone** — the existing directory is registered as `main_repo_path`.
3. Otherwise: run `git clone` as the target OS user, then register the project.

## API (ConnectionService)

- `ListProjects` / `CreateProject` — manage projects.
- `StartSession` takes **`project_id`** (replaces ad-hoc `repo_path`); the daemon resolves the working directory from the project’s `main_repo_path`.
- `ListSessions` returns `project_id` per session.

## Session metadata

`tddy-core` `SessionMetadata` includes required **`project_id`**. Spawns pass **`--project-id`** to `tddy-coder`. Older `.session.yaml` files without `project_id` are skipped when listing (breaking change).

## Related

- [PRD: tddy-daemon (WIP)](1-WIP/PRD-2026-03-19-tddy-daemon.md) — multi-user daemon (tooling, auth, spawn); connection UX is project-based as documented here.
- [Web terminal](../web/web-terminal.md) — Connection screen UI.
