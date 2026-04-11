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
| `main_branch_ref` | Optional. Remote-tracking ref used as the integration base for worktree fetch and checkout (e.g. `origin/main`, `origin/master`). Omitted rows use **`origin/master`** as the documented default at resolution time. |
| `host_repo_paths` | Per-host (or per-daemon-instance) checkout paths keyed by host key; see multi-host daemon docs. |

## Storage

- **Projects registry:** `~/.tddy/projects/projects.yaml` (list of projects).
- **Clone location (default):** `{home}/{repos_base_path}/{name}/` where `repos_base_path` comes from daemon config (default: `repos`).
- **Optional override:** `CreateProject` may set **`user_relative_path`** (POSIX path relative to the user’s home, e.g. `Code/my-app` or `~/Code/my-app`). When set, the clone destination is that path instead of `{repos_base}/{name}/`.

## Daemon configuration

```yaml
# Optional; default is "repos" under each user's home directory
repos_base_path: "repos"
```

## Create project behavior

1. Resolve destination: `{repos_base}/{name}/`, **unless** `user_relative_path` is non-empty — then `{home}/{user_relative_path}` (normalized; `..` and absolute paths are rejected).
2. **If that path already exists** (checked as the target OS user): **no clone** — the existing directory is registered as `main_repo_path`.
3. Otherwise: run `git clone` as the target OS user, then register the project.

## API (ConnectionService)

- `ListProjects` / `CreateProject` — manage projects.
- `StartSession` takes **`project_id`** (replaces ad-hoc `repo_path`); the daemon resolves the working directory from the project’s `main_repo_path`.
- `ListSessions` returns `project_id` per session.

## Session metadata

`tddy-core` `SessionMetadata` includes required **`project_id`**. Spawns pass **`--project-id`** to `tddy-coder`. Older `.session.yaml` files without `project_id` are skipped when listing (breaking change).

## Multi-user daemon (`tddy-daemon`)

The **`tddy-daemon`** binary is the multi-user orchestrator: serves the web bundle, exposes **AuthService** (GitHub OAuth via Connect-RPC), maps authenticated GitHub users to OS users, lists allowed tools and sessions, and spawns **`tddy-coder`** with LiveKit credentials and **`--project-id`** when applicable. **`tddy-coder --daemon`** remains for single-user local use. Service install and paths: [systemd-install.md](systemd-install.md). Connection UX: [Web terminal](../web/web-terminal.md).

## Related

- [Git integration base ref (worktrees)](../coder/git-integration-base-ref.md) — validation, default ref, project registry fields.
- [gRPC remote control](../coder/grpc-remote-control.md) — daemon and transport roles.
- [Web terminal](../web/web-terminal.md) — Connection screen UI.
- [LiveKit peer discovery and host selection](livekit-peer-discovery.md) — **`ListEligibleDaemons`**, **`StartSession`** routing across daemons sharing **`livekit.common_room`**.
