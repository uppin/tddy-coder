# ConnectionService (tddy-daemon)

Connect-RPC service for tools, sessions, and **projects** when using `tddy-web` in **daemon mode**.

## Endpoints

| RPC | Purpose |
|-----|---------|
| `ListTools` | Allowed `tddy-*` binaries from config |
| `ListSessions` | Sessions under `~/.tddy/sessions/` with `.session.yaml` (includes `project_id`) |
| `ListProjects` | Projects from `~/.tddy/projects/projects.yaml` |
| `CreateProject` | Clone (or adopt existing path) + append registry |
| `StartSession` | Resolve `project_id` → `main_repo_path`, spawn tool with `--project-id` |
| `ConnectSession` / `ResumeSession` | LiveKit / respawn (resume passes `project_id` from metadata) |

## Paths (per mapped OS user)

| Purpose | Path |
|---------|------|
| Sessions | `~/.tddy/sessions/<session_id>/` |
| Projects file | `~/.tddy/projects/projects.yaml` |
| Clone default | `~/{repos_base_path}/{name}/` where `repos_base_path` comes from config (default `repos`) |
| `CreateProject.user_relative_path` | Optional: clone/adopt at `~/<path>` instead (e.g. `Code/foo` or `~/Code/foo`); must stay under home |

## Spawn worker

Spawn and **git clone** requests run through a forked single-threaded worker (`spawn_worker`) so fork+setuid from a Tokio process avoids deadlocks. JSON protocol: `WorkerRequest` (`spawn` | `clone`) and `WorkerResponse` (`spawn_ok` | `clone_ok` | `error`).

## See also

- Feature: [docs/ft/daemon/project-concept.md](../../../../docs/ft/daemon/project-concept.md)
- [changesets.md](./changesets.md)
