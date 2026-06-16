# ConnectionService (tddy-daemon)

Connect-RPC service for tools, sessions, and **projects** when using `tddy-web` in **daemon mode**.

## Endpoints

| RPC | Purpose |
|-----|---------|
| `ListTools` | Allowed `tddy-*` binaries from config (`allowed_tools`) |
| `ListAgents` | Allowed coding backends from config (`allowed_agents`): each entry has `id` (value for `StartSession.agent` / `tddy-coder --agent`) and `label` (display string; optional YAML `label` trimmed; blank or whitespace-only falls back to `id`) |
| `ListSessions` | Lists directories under `{sessions_base}/sessions/` that contain `.session.yaml` (includes `project_id` and `daemon_instance_id` for the owning daemon); each entry includes workflow fields populated from **`changeset.yaml`** when present (see below). **`sessions_base`** is the Tddy data directory for the mapped OS user (typically `~/.tddy`), so session trees are **`{sessions_base}/sessions/{session_id}/`**. |
| `ListProjects` | Builds the response from the local registry file **`~/.tddy/projects/projects.yaml`**, then appends optional extra rows from **`EligibleDaemonSource::peer_project_entries(session_token)`** (each **`ProjectEntry`** carries **`daemon_instance_id`** for the owning instance). The default **`EligibleDaemonSource`** supplies **no** peer rows; deployments that implement the trait may merge peer-supplied projects (merge semantics and partial failures are defined by that implementation). |
| `CreateProject` | Clone (or adopt existing path) + append registry |
| `ListEligibleDaemons` | Eligible daemon instances for host selection (`instance_id`, `label`, `is_local`); sourced from `EligibleDaemonSource` |
| `ListSessionWorkflowFiles` | Lists workflow file **basenames** present on disk under `{sessions_base}/sessions/{session_id}/` using a **fixed server allowlist** (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`). Requires the same **`session_token`** → user → **`sessions_base`** resolution as **`ListSessions`**; **`session_id`** is validated with **`validate_session_id_segment`** before path construction. Entries whose canonical path falls outside the canonical session directory (e.g. symlink escape) are omitted from the list. |
| `ReadSessionWorkflowFile` | Returns UTF-8 text for one allowlisted **basename** under the same resolved session directory. Rejects empty, non-allowlisted, or path-segment-unsafe **`basename`** values (`..`, `/`, `\`). Uses canonical path checks so resolved file paths cannot sit outside the session root. |
| `StartSession` | Resolve `project_id` → `main_repo_path`, spawn tool with `--project-id`; optional `daemon_instance_id` selects target instance (local spawn when empty or local; non-local targets are unsupported until cross-daemon routing exists). When **`allowed_agents`** in config is non-empty, a non-empty **`agent`** on the request must match an entry **`id`** (after trim); otherwise the RPC returns **`INVALID_ARGUMENT`**. When **`allowed_agents`** is empty, **`agent`** is not restricted by this allowlist. When `session_type == "claude-cli"`, the tool-spawn path is bypassed entirely — see [Claude Code CLI sessions](#claude-code-cli-sessions) below. |
| `ConnectSession` / `ResumeSession` | LiveKit / respawn (resume passes `project_id` from metadata); `session_id` is validated as a single path segment before resolving `{sessions_base}/sessions/{session_id}/`. For `session_type == "claude-cli"` sessions, `ConnectSession` returns empty LiveKit fields immediately (no token RPC). |
| `StreamSessionTerminalIO` | Bidi stream for raw terminal I/O with a running `claude` CLI process. First client message must carry `session_token` + `session_id` for auth. Subsequent messages carry raw stdin bytes; the server forwards them to the child process stdin and broadcasts stdout/stderr back as `SessionTerminalOutput` messages. Resize: if the input starts with `\x1b]resize;{cols};{rows}\x07`, the daemon updates the terminal size instead of forwarding to stdin. Session must have `session_type == "claude-cli"`; returns `FAILED_PRECONDITION` when no active process is found. |
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

Session **status** strings in metadata drive workflow display; optional Telegram notifications keyed on status transitions are documented in **[telegram-notifier.md](./telegram-notifier.md)** (product context: **[telegram-notifications.md](../../../docs/ft/daemon/telegram-notifications.md)**).

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

## Claude Code CLI sessions

When `StartSessionRequest.session_type == "claude-cli"`, the standard tool-spawn path is bypassed. Instead:

1. A dedicated git worktree is created under `{sessions_base}/worktrees/claude-cli-{short_id}/` (branch `claude-cli/{short_id}`).
2. `.session.yaml` is written with `session_type = "claude-cli"` and `model = <requested model>`. `repo_path` is set to the worktree path for later cleanup.
3. `ClaudeCliSessionManager::start()` spawns the `claude` binary (resolved from `claude_cli.binary_path` in config, default `"claude"`) with `--model <model> --session-id <session_id>` in the worktree directory.
4. `StartSessionResponse` returns empty LiveKit fields (`livekit_room`, `livekit_url`, `livekit_server_identity` all empty); the web client detects this and routes to `ConnectedClaudeCliTerminal`.

**`ClaudeCliSessionManager`** (`claude_cli_session.rs`): in-memory registry (`HashMap<String, Arc<PtyHandle>>`) mapping session id to the active child process. `PtyHandle` holds:
- `stdin_tx`: `mpsc::UnboundedSender<Bytes>` — feed bytes into the child stdin
- `stdout_tx`: `broadcast::Sender<Bytes>` — subscribe for stdout/stderr chunks
- `worktree_path`: for deletion
- `pid`: for signal delivery

A background task monitors the child with `child.wait()`; on exit the entry is removed from the registry. `resume()` calls `start()` in the same worktree — the worktree's file state is preserved.

**`StreamSessionTerminalIO`**: The bidi gRPC stream calls `ClaudeCliSessionManager::get(session_id)` to look up the live `PtyHandle`. A write task reads `SessionTerminalInput` messages from the client stream and sends bytes to `stdin_tx`. A read task subscribes to `stdout_tx` and forwards chunks as `SessionTerminalOutput` to the client stream. Resize sequences (`\x1b]resize;...`) are intercepted before forwarding (no actual pty resize is performed; the sequence is dropped). Auth: `session_token` validated on the first message via the same GitHub → OS user path as other RPCs.

**`DeleteSession` for claude-cli**: After PID termination (SIGTERM / SIGKILL), the daemon also calls `remove_dir_all` on the worktree path stored in `metadata.repo_path`. The session directory is then removed as usual.

**`ReportSessionStatus`**: Hook-driven RPC. `tddy-tools session-hook` calls this after mapping a Claude Code lifecycle event to a `SessionActivityStatus`. The handler validates `session_id` (path traversal guard), resolves `sessions_base` from `os_user` directly (no web-token path), reads `.session.yaml`, requires `session_type == "claude-cli"`, constant-time-compares `hook_token`, then calls `update_activity_status(session_dir, status)`. Sessions with `hook_token: None` (e.g. Telegram-started) return `PermissionDenied`. See [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md#session-activity-status-via-per-worktree-hooks) for the full hook flow.

**`config.rs`**: Optional `claude_cli:` block:

```yaml
claude_cli:
  binary_path: /usr/local/bin/claude   # default: "claude" (PATH lookup)
  tddy_tools_path: /usr/local/bin/tddy-tools  # default: current_exe sibling → "tddy-tools"
  daemon_url: http://127.0.0.1:8899    # default: http://127.0.0.1:{web_port}
```

## Spawn worker

Spawn and **git clone** requests run through a forked single-threaded worker (`spawn_worker`) so fork+setuid from a Tokio process avoids deadlocks. JSON protocol: `WorkerRequest` (`spawn` | `clone`) and `WorkerResponse` (`spawn_ok` | `clone_ok` | `error`).

## See also

- **Worktrees**: **`ListWorktreesForProject`** (cached rows; **`refresh`** runs **`WorktreeStatsCache::refresh_stats_for_project`** in a blocking worker), **`RemoveWorktree`** ( **`remove_worktree_under_repo`**, then **`invalidate_project`**). Project checkout: **`main_repo_path_for_host`** with the local **`daemon_instance_id`**. Details: [worktrees.md](./worktrees.md), [docs/ft/web/worktrees.md](../../../../docs/ft/web/worktrees.md).
- Feature: [Session directory layout](../../../../docs/ft/coder/session-layout.md)
- Feature: [docs/ft/daemon/project-concept.md](../../../../docs/ft/daemon/project-concept.md)
- [changesets.md](./changesets.md)
