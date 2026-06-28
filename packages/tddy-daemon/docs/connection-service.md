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
| `StreamSessionTerminalIO` | Bidi stream for raw terminal I/O with a running `claude` CLI process. First client message must carry `session_token` + `session_id` for auth. Subsequent messages carry raw stdin bytes; the server forwards them to the child process stdin and broadcasts stdout/stderr back as `SessionTerminalOutput` messages. Resize: if the input starts with `\x1b]resize;{cols};{rows}\x07`, the daemon updates the terminal size instead of forwarding to stdin. Session must have `session_type == "claude-cli"`; returns `FAILED_PRECONDITION` when no active process is found. Accepts an optional `terminal_id` on the first message (empty ⇒ the reserved `"main"` claude terminal); an unknown id returns `NOT_FOUND`. |
| `StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions` | Manage the **tools** running in a session — see [Session tools](#session-tools-multiple-terminals-per-session) below. |
| `ExecuteTool` | Runs one exec-tool (Read, Write, StrReplace, Delete, Grep, Glob, Shell, Await, ReadLints, SemanticSearch) against the session's worktree. After execution, appends a `ToolCallRecord` to the durable JSONL log `~/.tddy/sessions/{session_id}/tool-calls.jsonl` (non-fatal: a write failure is logged as a warning and never blocks the response). Authenticates via `session_token` → OS user, validates `session_id`; optional `daemon_instance_id` for peer routing. |
| `ListExecTools` | Returns the exec-tool catalog (`ToolDef` per tool: `name`, `description`, `input_schema_json`). Auth same as `ExecuteTool`. |
| `ListSessionToolCalls` | Returns the durable tool-call log for a session (up to 500 most-recent entries from `tool-calls.jsonl`; ordered chronologically). Each `ToolCallInfo` carries `task_id`, `tool_name`, `args_json`, `result_json`, `is_error`, `error_message`, `job_running`, `created_unix_ms`. Authenticates via `session_token`, validates `session_id` (path-segment guard), optionally routes to owning daemon via `daemon_instance_id`. |
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

## Sandboxed Claude Code CLI sessions

When `StartSessionRequest.session_type == "claude-cli"` **and** `sandbox == true` on macOS, the daemon uses the sandbox spawn path instead of `ClaudeCliSessionManager::start()`:

1. Creates the same git worktree as a non-sandbox claude-cli session (host `tool_engine::execute_tool` operates on this worktree).
2. Prepares a read-only **context dir** (`SandboxContextDir`: synced `CLAUDE.md`/`AGENTS.md`/skills + `REMOTE_APPENDIX`).
3. Renders an SBPL profile and spawns `tddy-tools sandbox-runner` via `sandbox-exec` (`tddy-sandbox-darwin`).
4. Waits for the in-jail gRPC ready marker, then **`dial_and_bridge`** on a single bidi **`SessionChannel`** (`sandbox_session.rs`).
5. Writes `.session.yaml` with `sandbox: true`; returns empty LiveKit fields.

**`SessionChannel`** (`packages/tddy-service/proto/sandbox.proto`) multiplexes PTY output, MCP tool exec, and LLM egress on one host-poll-driven bidi stream:

| Host → sandbox | Sandbox → host |
|----------------|----------------|
| `SubscribeTerminal`, `HostPoll`, `SandboxInput`, `ExecuteToolResponse`, `EgressResponse`, `TunnelOpenAck`, `TunnelData`, `TunnelClose` | `SessionTerminalOutput`, `ExecuteToolRequest`, `EgressRequest`, `TunnelOpen`, `TunnelData`, `TunnelClose` |

Outbound network from the jail is **`(deny network*)`** — the sandbox never dials out. The agent reaches the network through an **in-jail HTTPS_PROXY CONNECT tunnel**: the runner exports `HTTPS_PROXY`/`HTTP_PROXY` to the `claude` PTY pointing at the loopback egress shim; `claude` issues `CONNECT api.anthropic.com:443`; the shim relays the raw (still TLS-encrypted) bytes over `SessionChannel` `TunnelOpen`/`TunnelData`/`TunnelClose` frames; the **host** (`sandbox_session.rs::spawn_tunnel`) opens the real outbound socket and pumps bytes both ways. TLS stays end-to-end, so the host never sees plaintext or credentials. The legacy unary `EgressRequest`/`EgressResponse` path (host `reqwest` fetch) is retained only for the `GET /probe` connectivity check.

> **Read confinement:** the SBPL profile is rendered from an explicit `SandboxPlan` read allow-list (`render_plan`) with **no** `(allow file-read*)` wildcard. The Claude read recipe (`tddy-sandbox/src/claude_spawn.rs`: `claude_required_reads`/`system_baseline_reads`) enumerates exactly what the V8/Node `claude` binary needs (dyld root `/`, system libs, ICU/timezone data, toolchain, the binary's `otool -L` deps, PTY devices). Both read and write are confined.

> **Status:** the egress tunnel is wired in the shared `runner.rs` + `sandbox_session.rs` helpers and validated for the `tddy-sandbox-app` host path (acceptance: `sandbox_runner_tunnels_https_proxy_connect_via_session_channel`). End-to-end validation through the daemon `StartSession` (`sandbox=true`) flow is **pending** (the runtime code is shared, but no daemon-specific egress acceptance test yet).

**In-jail runner** (`tddy-tools sandbox-runner`): binds loopback gRPC, spawns `claude` in a PTY with `mcp__tddy-tools__*` allowlist (`sandbox_claude_spawn.rs`), routes MCP `call_tool` through tool IPC → relay queue → `ExecuteToolRequest` on `HostPoll`.

**Terminal I/O**: `StreamTerminalOutput` / `SendTerminalInput` on the daemon delegate to `SandboxSessionManager` when `metadata.sandbox == true`.

**Lifecycle**:
- **`DeleteSession`**: stops the `SandboxHandle` (SIGTERM → SIGKILL), removes worktree and session dir.
- **`ResumeSession`**: `relaunch_sandboxed_runner()` respawns the jail process and re-dials `SessionChannel`; worktree is reused.

**Non-macOS**: `tddy-sandbox` returns `Unsupported`; the RPC maps to `failed_precondition` (no fallback).

**Seatbelt troubleshooting**: [tddy-sandbox-darwin troubleshooting](../../../packages/tddy-sandbox-darwin/docs/troubleshooting.md). Agent skill: [.agents/skills/darwin-sandbox/SKILL.md](../../../../.agents/skills/darwin-sandbox/SKILL.md).

**`config.rs`**: Optional `claude_cli:` block:

```yaml
claude_cli:
  binary_path: /usr/local/bin/claude   # default: "claude" (PATH lookup)
  tddy_tools_path: /usr/local/bin/tddy-tools  # default: current_exe sibling → "tddy-tools"
  daemon_url: http://127.0.0.1:8899    # default: http://127.0.0.1:{web_port}
```

## Session tools (multiple terminals per session)

A session can run multiple identified **tools**, each a `PtyHandle` in `ClaudeCliSessionManager`'s
two-level registry `session_id → (terminal_id → PtyHandle)`. The original `claude` process is the
tool under the reserved id `MAIN_TERMINAL_ID` (`"main"`, kind `"claude-cli"`); additional **Bash**
tools (kind `"bash"`) run the user's `$SHELL` (fallback `/bin/bash`) in the session worktree and
take no inputs.

- `StartTerminalSession(session_id)` — resolves the worktree from the running `"main"` terminal
  (`FAILED_PRECONDITION` if none), spawns a Bash tool, returns a fresh `terminal_id` (uuid v7).
- `StopTerminalSession(session_id, terminal_id)` — SIGTERM→SIGKILL the tool's pid and deregister
  (idempotent with the PTY exit-monitor). Rejects `terminal_id == "main"` with `INVALID_ARGUMENT`
  (use `SignalSession`/`DeleteSession` for the session itself); unknown id → `NOT_FOUND`.
- `ListTerminalSessions(session_id)` — `TerminalSessionInfo{terminal_id, kind, pid}` per running tool.

The terminal I/O RPCs (`StreamSessionTerminalIO`, `StreamTerminalOutput`, `SendTerminalInput`)
carry an optional `terminal_id` (empty ⇒ `"main"`) and resolve the target via
`get_terminal(session_id, terminal_id)`. All four new/extended RPCs authenticate `session_token`
via the same GitHub → OS user path as the other endpoints.

## Spawn worker

Spawn and **git clone** requests run through a forked single-threaded worker (`spawn_worker`) so fork+setuid from a Tokio process avoids deadlocks. JSON protocol: `WorkerRequest` (`spawn` | `clone`) and `WorkerResponse` (`spawn_ok` | `clone_ok` | `error`).

## Durable tool-call log

Every `ExecuteTool` invocation appends a JSON line to **`~/.tddy/sessions/{session_id}/tool-calls.jsonl`** (module **`tool_call_log.rs`**):

```json
{"task_id":"...","tool_name":"Read","args_json":"{\"path\":\"/foo\"}","result_json":"...","is_error":false,"error_message":"","job_running":false,"created_unix_ms":1719312000000}
```

- **Append-only** — no read-modify-write; safe for concurrent invocations.
- **Durability** — survives daemon restarts and in-memory registry eviction (5-min TTL).
- **Scope** — one file per session directory; calls from other sessions are not included.
- **Tail cap** — `read_tool_calls` returns at most the 500 most-recent records; malformed lines are skipped with a warning.
- **Non-fatal** — a write error only produces a `warn!` log entry; the `ExecuteTool` response is unaffected.
- **Background Shell** — jobs with `job_running: true` store `task_id`; live stdout/stderr is available via `TaskService.WatchTask` while the task is in the registry. Durable stdio for detached jobs is not captured in the log.

`ListSessionToolCalls` reads this file and returns up to 500 `ToolCallInfo` records in chronological order (UI reverses to newest-first).

## See also

- **Worktrees**: **`ListWorktreesForProject`** (cached rows; **`refresh`** runs **`WorktreeStatsCache::refresh_stats_for_project`** in a blocking worker), **`RemoveWorktree`** ( **`remove_worktree_under_repo`**, then **`invalidate_project`**). Project checkout: **`main_repo_path_for_host`** with the local **`daemon_instance_id`**. Details: [worktrees.md](./worktrees.md), [docs/ft/web/worktrees.md](../../../../docs/ft/web/worktrees.md).
- Feature: [Session directory layout](../../../../docs/ft/coder/session-layout.md)
- Feature: [docs/ft/daemon/project-concept.md](../../../../docs/ft/daemon/project-concept.md)
- [changesets.md](./changesets.md)
