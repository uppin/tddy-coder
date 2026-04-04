# ConnectionService (tddy-daemon)

Connect-RPC service for tools, sessions, and **projects** when using `tddy-web` in **daemon mode**.

## Endpoints

| RPC | Purpose |
|-----|---------|
| `ListTools` | Allowed `tddy-*` binaries from config (`allowed_tools`) |
| `ListAgents` | Allowed coding backends from config (`allowed_agents`): each entry has `id` (value for `StartSession.agent` / `tddy-coder --agent`) and `label` (display string; optional YAML `label` trimmed; blank or whitespace-only falls back to `id`) |
| `ListSessions` | Lists directories under `{sessions_base}/sessions/` that contain `.session.yaml` (includes `project_id` and `daemon_instance_id` for the owning daemon); each entry includes workflow fields populated from **`changeset.yaml`** when present (see below). **`sessions_base`** is the Tddy data directory for the mapped OS user (typically `~/.tddy`), so session trees are **`{sessions_base}/sessions/{session_id}/`**. |
| `ListProjects` | Projects from `~/.tddy/projects/projects.yaml` |
| `CreateProject` | Clone (or adopt existing path) + append registry |
| `ListEligibleDaemons` | Eligible daemon instances for host selection (`instance_id`, `label`, `is_local`); sourced from `EligibleDaemonSource` |
| `ListSessionWorkflowFiles` | Lists workflow file **basenames** present on disk under `{sessions_base}/sessions/{session_id}/` using a **fixed server allowlist** (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`). Requires the same **`session_token`** → user → **`sessions_base`** resolution as **`ListSessions`**; **`session_id`** is validated with **`validate_session_id_segment`** before path construction. Entries whose canonical path falls outside the canonical session directory (e.g. symlink escape) are omitted from the list. |
| `ReadSessionWorkflowFile` | Returns UTF-8 text for one allowlisted **basename** under the same resolved session directory. Rejects empty, non-allowlisted, or path-segment-unsafe **`basename`** values (`..`, `/`, `\`). Uses canonical path checks so resolved file paths cannot sit outside the session root. |
| `StartSession` | Resolve `project_id` → `main_repo_path`, spawn tool with `--project-id`; optional `daemon_instance_id` selects target instance (local spawn when empty or local; non-local targets are unsupported until cross-daemon routing exists). When **`allowed_agents`** in config is non-empty, a non-empty **`agent`** on the request must match an entry **`id`** (after trim); otherwise the RPC returns **`INVALID_ARGUMENT`**. When **`allowed_agents`** is empty, **`agent`** is not restricted by this allowlist. |
| `ConnectSession` / `ResumeSession` | LiveKit / respawn (resume passes `project_id` from metadata); `session_id` is validated as a single path segment before resolving `{sessions_base}/sessions/{session_id}/` |
| `DeleteSession` | Removes **`{sessions_base}/sessions/{session_id}/`**. If **`.session.yaml`** records a live PID, the daemon sends **SIGTERM**, waits, then **SIGKILL** as needed (Linux zombie sessions are treated as stopped), then removes the directory. Directories without readable metadata are still removed when the path resolves safely. Rejects unknown ids and path-unsafe ids (implementation in **`session_deletion`**) |
| `SignalSession` | Send Unix signal to recorded PID for an active session; `session_id` validated before path resolution |

## ListSessions workflow fields

For each session directory, the daemon merges **`.session.yaml`** with optional **`changeset.yaml`**:

- **`workflow_goal`**: Session row **tag** for the row whose **id** matches **`.session.yaml`** **`session_id`**.
- **`workflow_state`**: **`changeset.state.current`** (string form of the workflow state).
- **`agent`**: Matching session row **agent**.
- **`model`**: **`changeset.models[tag]`** when the map contains that tag.
- **`elapsed_display`**: Compact duration from **`tddy_core::format_elapsed_compact`**, using wall time since the last **`state.history`** entry whose state matches **`state.current`**, or **`state.updated_at`** when no matching history entry exists.

If the changeset is missing, unreadable, or has no matching session row, the corresponding fields use **placeholders** (em dash) or partial data as implemented in **`session_list_enrichment`**.

The directory listing and enrichment execute inside **`spawn_blocking_with_timeout`** so the async RPC handler does not block the Tokio runtime on disk I/O.

## Session workflow file RPCs

- **Implementation**: Filesystem policy and I/O live in **`session_workflow_files`**. **`ListSessionWorkflowFiles`** and **`ReadSessionWorkflowFile`** authenticate like other session-scoped RPCs, resolve **`unified_session_dir_path`**, and operate only on the fixed basename allowlist.
- **Listing**: Returns only allowlisted names for paths that exist, resolve under the canonical session directory, and pass **`is_file`** checks.
- **Reading**: Reads with **`std::fs::read_to_string`** after canonical path verification. Responses are not size-capped at the API layer; operators should keep workflow files within reasonable size for the deployment.
- **Async handler note**: Handlers are **`async`** but perform blocking filesystem work on the runtime thread; volume is expected to stay low (dashboard use). Heavy concurrency may warrant moving work behind **`spawn_blocking`**.
- **Tests**: Integration coverage in **`session_workflow_files_rpc`**.

## DeleteSession behavior

- **Auth**: Same `session_token` → GitHub user → mapped OS user → `sessions_base` as `ListSessions`.
- **Safety**: Session id is a single path segment (no `/`, `..`, or separators); resolved directory must sit under **`{sessions_base}/sessions/`** via **`unified_session_dir_path`**.
- **Process termination**: When **`metadata.pid`** is set and the process is still running on Unix, the daemon terminates it (**SIGTERM**, then **SIGKILL** if needed) before **`remove_dir_all`**. Zombies on Linux are detected so delete can finish even when the parent has not reaped the child.
- **Metadata gaps**: If **`.session.yaml`** is missing or unreadable, the directory is still removed when present (no PID termination step).
- **Errors**: Invalid id → `INVALID_ARGUMENT`; missing directory on this daemon → `FAILED_PRECONDITION` (routing); process still running after signals → `FAILED_PRECONDITION`; filesystem removal failure → `INTERNAL` with a generic client message; details are logged server-side.

## Paths (per mapped OS user)

| Purpose | Path |
|---------|------|
| Sessions | `{sessions_base}/sessions/<session_id>/` where **`sessions_base`** is the Tddy data root (typically **`~/.tddy`**, same layout as **`tddy-coder`** artifacts) |
| Projects file | `~/.tddy/projects/projects.yaml` |
| Clone default | `~/{repos_base_path}/{name}/` where `repos_base_path` comes from config (default `repos`) |
| `CreateProject.user_relative_path` | Optional: clone/adopt at `~/<path>` instead (e.g. `Code/foo` or `~/Code/foo`); must stay under home |

Project rows in **`projects.yaml`** may include optional **`main_branch_ref`** (`origin/<branch>`). **`effective_integration_base_ref_for_project`** in **`project_storage`** returns that value or the documented default **`origin/master`**. Invalid values fail **`add_project`** before the file is written. See [git-integration-base-ref.md](../../../../docs/ft/coder/git-integration-base-ref.md) and [project-concept.md](../../../../docs/ft/daemon/project-concept.md).

## Spawn worker

Spawn and **git clone** requests run through a forked single-threaded worker (`spawn_worker`) so fork+setuid from a Tokio process avoids deadlocks. JSON protocol: `WorkerRequest` (`spawn` | `clone`) and `WorkerResponse` (`spawn_ok` | `clone_ok` | `error`).

## See also

- Feature: [Session directory layout](../../../../docs/ft/coder/session-layout.md)
- Feature: [docs/ft/daemon/project-concept.md](../../../../docs/ft/daemon/project-concept.md)
- [changesets.md](./changesets.md)
