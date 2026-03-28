# ConnectionService (tddy-daemon)

Connect-RPC service for tools, sessions, and **projects** when using `tddy-web` in **daemon mode**.

## Endpoints

| RPC | Purpose |
|-----|---------|
| `ListTools` | Allowed `tddy-*` binaries from config |
| `ListSessions` | Lists directories under `{sessions_base}/sessions/` that contain session metadata (includes `project_id` and `daemon_instance_id` for the owning daemon) |
| `ListProjects` | Projects from `~/.tddy/projects/projects.yaml` |
| `CreateProject` | Clone (or adopt existing path) + append registry |
| `ListEligibleDaemons` | Eligible daemon instances for host selection (`instance_id`, `label`, `is_local`); sourced from `EligibleDaemonSource` |
| `StartSession` | Resolve `project_id` → `main_repo_path`, spawn tool with `--project-id`; optional `daemon_instance_id` selects target instance (local spawn when empty or local; non-local targets are unsupported until cross-daemon routing exists) |
| `ConnectSession` / `ResumeSession` | LiveKit / respawn (resume passes `project_id` from metadata); `session_id` is validated as a single path segment before resolving `{sessions_base}/sessions/{session_id}/` |
| `DeleteSession` | Removes `~/.tddy/sessions/<session_id>/` when the session is inactive (PID in metadata not alive); rejects active sessions, unknown ids, and path-unsafe ids (implementation in `session_deletion`) |
| `SignalSession` | Send Unix signal to recorded PID for an active session; `session_id` validated before path resolution |

## DeleteSession behavior

- **Auth**: Same `session_token` → GitHub user → mapped OS user → `sessions_base` as `ListSessions`.
- **Safety**: Session id is a single path segment (no `/`, `..`, or separators); resolved directory must sit directly under `sessions_base`.
- **Inactive check**: Matches session listing: `is_active` is true only when `metadata.pid` is present and the process is alive (`kill(pid, 0)` on Unix).
- **Errors**: Invalid id → `INVALID_ARGUMENT`; missing directory or unreadable metadata → `NOT_FOUND`; live PID → `FAILED_PRECONDITION`; filesystem removal failure → `INTERNAL` with a generic client message; details are logged server-side.

## Paths (per mapped OS user)

| Purpose | Path |
|---------|------|
| Sessions | `{sessions_base}/sessions/<session_id>/` (default user base includes `~/.tddy`) |
| Projects file | `~/.tddy/projects/projects.yaml` |
| Clone default | `~/{repos_base_path}/{name}/` where `repos_base_path` comes from config (default `repos`) |
| `CreateProject.user_relative_path` | Optional: clone/adopt at `~/<path>` instead (e.g. `Code/foo` or `~/Code/foo`); must stay under home |

## Spawn worker

Spawn and **git clone** requests run through a forked single-threaded worker (`spawn_worker`) so fork+setuid from a Tokio process avoids deadlocks. JSON protocol: `WorkerRequest` (`spawn` | `clone`) and `WorkerResponse` (`spawn_ok` | `clone_ok` | `error`).

## See also

- Feature: [Session directory layout](../../../../docs/ft/coder/session-layout.md)
- Feature: [docs/ft/daemon/project-concept.md](../../../../docs/ft/daemon/project-concept.md)
- [changesets.md](./changesets.md)
