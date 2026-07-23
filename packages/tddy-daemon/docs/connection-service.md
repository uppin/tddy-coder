# ConnectionService (tddy-daemon)

Connect-RPC service for tools, sessions, and **projects** when using `tddy-web` in **daemon mode**.

## Endpoints

| RPC | Purpose |
|-----|---------|
| `ListTools` | Allowed `tddy-*` binaries from config (`allowed_tools`) |
| `ListAgents` | Allowed coding backends from config (`allowed_agents`): each entry has `id` (value for `StartSession.agent` / `tddy-coder --agent`) and `label` (display string; optional YAML `label` trimmed; blank or whitespace-only falls back to `id`) |
| `ListSubagents` | Resolved specialized-agent defs (builtin `fastcontext` + `<tddyhome>/agents/*.yaml` — see [specialized-subagents.md](../../../docs/ft/coder/specialized-subagents.md)): each `SubagentInfo` carries `name` (value for `StartSession.specialized_agents` entries), `label` (blank falls back to `name`), and `model`. |
| `ListSessions` | Lists directories under `{sessions_base}/sessions/` that contain `.session.yaml` (includes `project_id` and `daemon_instance_id` for the owning daemon); each entry includes workflow fields populated from **`changeset.yaml`** when present (see below). **`sessions_base`** is the Tddy data directory for the mapped OS user (typically `~/.tddy`), so session trees are **`{sessions_base}/sessions/{session_id}/`**. |
| `ListProjects` | Builds the response from the local registry file **`~/.tddy/projects/projects.yaml`**, then — unless **`local_only`** is set — appends peer rows from **`EligibleDaemonSource::peer_project_entries(session_token)`** (each **`ProjectEntry`** carries **`daemon_instance_id`** for the owning instance). **`local_only = true`** returns only the local rows and skips fan-out; it is set by the peer-aggregation path itself to prevent recursion. The LiveKit-backed **`LiveKitEligibleDaemonSource`** fans out to each discovered peer's **`ListProjects`** (with **`local_only = true`**), tags returned rows with the peer's instance id, and logs+skips unreachable peers. |
| `CreateProject` | Clone (or adopt existing path) + append registry (mints a fresh `project_id`) |
| `SetProjectDefaultBranch` | Sets a project's stored default branch (**`main_branch_ref`**) via **`project_storage::set_project_default_branch`**. Validates the ref shape (rejecting unsafe input → `INVALID_ARGUMENT`) and project existence (→ `NOT_FOUND`) before any write; returns the updated **`ProjectEntry`**. Routes by target **`daemon_instance_id`** like **`AddProjectToHost`** (empty/local = local write; a peer = forward via **`forward_set_project_default_branch_via_livekit`**), so the default is a property of the logical project across hosts. See [projects-screen-multi-host.md](../../../docs/ft/web/projects-screen-multi-host.md#default-branch). |
| `AddProjectToHost` | Makes an existing project available on another host, **reusing its `project_id`**. Routes by target **`daemon_instance_id`** (empty/local = handle locally; a peer = forward over the LiveKit common room via **`forward_add_project_to_host_via_livekit`**, same `classify_peer_route` routing as **`StartSession`**). The handling daemon clones the repo (like **`CreateProject`**) and persists a **`projects.yaml`** row with the **given** `project_id` via **`project_storage::add_or_get_project`** — **idempotent**: if the host already registers that id, the existing row is returned with no re-clone. Rejects blank `project_id`/`name`/`git_url` (`INVALID_ARGUMENT`) and unknown/unreachable target hosts (`FAILED_PRECONDITION`). See [projects-screen-multi-host.md](../../../docs/ft/web/projects-screen-multi-host.md). |
| `ListEligibleDaemons` | Eligible daemon instances for host selection (`instance_id`, `label`, `is_local`); sourced from `EligibleDaemonSource` |
| `ListSessionWorkflowFiles` | Lists workflow file **basenames** present on disk under `{sessions_base}/sessions/{session_id}/` using a **fixed server allowlist** (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`). Requires the same **`session_token`** → user → **`sessions_base`** resolution as **`ListSessions`**; **`session_id`** is validated with **`validate_session_id_segment`** before path construction. Entries whose canonical path falls outside the canonical session directory (e.g. symlink escape) are omitted from the list. |
| `ReadSessionWorkflowFile` | Returns UTF-8 text for one allowlisted **basename** under the same resolved session directory. Rejects empty, non-allowlisted, or path-segment-unsafe **`basename`** values (`..`, `/`, `\`). Uses canonical path checks so resolved file paths cannot sit outside the session root. |
| `StartSession` | Resolve `project_id` → `main_repo_path`, spawn tool with `--project-id`; optional `daemon_instance_id` selects target instance (local spawn when empty or local; non-local targets are unsupported until cross-daemon routing exists). For a new-branch-from-base worktree with an empty `selected_integration_base_ref`, the base ref is the project's stored **`main_branch_ref`** when set; a legacy project (no stored default) falls through to worktree setup's live default resolution — so the project default applies to web sessions, not only Telegram. When **`allowed_agents`** in config is non-empty, a non-empty **`agent`** on the request must match an entry **`id`** (after trim); otherwise the RPC returns **`INVALID_ARGUMENT`**. When **`allowed_agents`** is empty, **`agent`** is not restricted by this allowlist. When `session_type == "claude-cli"` or `"cursor-cli"`, the tool-spawn path is bypassed — see [Claude Code CLI sessions](#claude-code-cli-sessions) and [Cursor Agent CLI sessions](#cursor-agent-cli-sessions). |
| `ConnectSession` / `ResumeSession` | LiveKit / respawn (resume passes `project_id` from metadata); `session_id` is validated as a single path segment before resolving `{sessions_base}/sessions/{session_id}/`. For `session_type == "claude-cli"` or `"cursor-cli"` sessions, `ConnectSession` returns empty LiveKit fields immediately (no token RPC). |
| `StreamSessionTerminalIO` | Bidi stream for raw terminal I/O with a running CLI child (`claude` or Cursor Agent CLI). First client message must carry `session_token` + `session_id` for auth. Subsequent messages carry raw stdin bytes; the server forwards them to the child process stdin and broadcasts stdout/stderr back as `SessionTerminalOutput` messages. Resize: if the input starts with `\x1b]resize;{cols};{rows}\x07`, the daemon updates the terminal size instead of forwarding to stdin. Session must have `session_type == "claude-cli"` or `"cursor-cli"`; returns `FAILED_PRECONDITION` when no active process is found. Accepts an optional `terminal_id` on the first message (empty ⇒ the reserved `"main"` terminal); an unknown id returns `NOT_FOUND`. |
| `StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions` | Manage the **tools** running in a session — see [Session tools](#session-tools-multiple-terminals-per-session) below. |
| `ExecuteTool` | Runs one exec-tool (Read, Write, StrReplace, Delete, Grep, Glob, Shell, Await, ReadLints, SemanticSearch) against the session's worktree. After execution, appends a `ToolCallRecord` to the durable JSONL log `~/.tddy/sessions/{session_id}/tool-calls.jsonl` (non-fatal: a write failure is logged as a warning and never blocks the response). Authenticates via `session_token` → OS user, validates `session_id`; optional `daemon_instance_id` for peer routing. |
| `ListExecTools` | Returns the exec-tool catalog (`ToolDef` per tool: `name`, `description`, `input_schema_json`). Auth same as `ExecuteTool`. |
| `ListSessionToolCalls` | Returns the durable tool-call log for a session (up to 500 most-recent entries from `tool-calls.jsonl`; ordered chronologically). Each `ToolCallInfo` carries `task_id`, `tool_name`, `args_json`, `result_json`, `is_error`, `error_message`, `job_running`, `created_unix_ms`. Authenticates via `session_token`, validates `session_id` (path-segment guard), optionally routes to owning daemon via `daemon_instance_id`. |
| `StreamSessionActivity` | Server-streaming feed of the **agent's own** tool calls (distinct from the human-triggered `ExecuteTool` log). On connect it replays the coalesced snapshot from `agent-activity.jsonl`, then tails live `AgentActivityRecord`s from the in-process per-session `AgentActivityHub`. Auth/validation like `ListSessionToolCalls`. **Local** routes only — `PeerRoute::Forward` returns `unimplemented` (streaming peer-forward is a follow-up; `forward_to_peer` is unary-only) rather than serving wrong-host data. See [Agent-activity log & stream](#agent-activity-log--stream). |
| `ReportAgentActivity` | Unary; the `tddy-tools session-hook` POSTs one record per Claude Code `PreToolUse`/`PostToolUse` (no shared id across hook processes, so the daemon pairs Pre→Post per session). Appends to `agent-activity.jsonl` and publishes to the hub. Auth like `report_session_status`; bad tokens rejected. |
| `DeleteSession` | Removes **`{sessions_base}/sessions/{session_id}/`**. If **`.session.yaml`** records a live PID, the daemon sends **SIGTERM**, waits, then **SIGKILL** as needed (Linux zombie sessions are treated as stopped), then removes the directory. Directories without readable metadata are still removed when the path resolves safely. Rejects unknown ids and path-unsafe ids (implementation in **`session_deletion`**) |
| `SignalSession` | Send Unix signal to recorded PID for an active session; `session_id` validated before path resolution |
| `AddPlannedPr` | Manually appends one planned PR to a **`"pr-stack"`** orchestrator session's **`Changeset.stack`**, with caller-chosen ancestors (**`StackNode.parents`**). Rejects (`FAILED_PRECONDITION`, via **`require_pr_stack_orchestrator`**) when **`session_id`**'s changeset `recipe` doesn't resolve to `"pr-stack"` (legacy aliases included); rejects a blank `title` or a dangling parent ref (`INVALID_ARGUMENT`). Delegates the actual DAG mutation to **`tddy_workflow_recipes::pr_stack::add_planned_pr_node`** (server-assigns `node_id`, cycle-checks, atomic append via `update_stack_atomic`). Response's `stack_plan_json` reuses the same serializer as `ListSessions` enrichment (`stack_plan_json_for_changeset`). See [pr-stacking.md § Manually adding a planned PR](../../../docs/ft/coder/pr-stacking.md#manually-adding-a-planned-pr). |

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

## Worktree Code pane file RPCs

Browse a session's **worktree** (the git checkout at `SessionEntry.repo_path`), not the session
metadata dir — powers the web [Code pane](../../../docs/ft/web/session-code-pane.md).

- **Implementation**: Filesystem/git policy lives in **`worktree_files`**. **`ListWorktreeDirectory`** and **`ReadWorktreeFile`** reuse the **`RemoveWorktree`** preamble (token → GitHub user → mapped OS user → project → **`main_repo_path_for_host`**), then gate on **`worktrees::worktree_path_is_listed`** so the `worktree_path` must appear in the project's `git worktree list`. The git/fs work runs inside **`spawn_blocking`** with `spawn_worker_request_timeout()`.
- **Listing** (`ListWorktreeDirectory`): one directory level at `rel_path` (empty = root). `.gitignore`-aware and `.git`-excluded via `git ls-files --cached --others --exclude-standard -z`; a linked worktree's private `<gitdir>/info/exclude` is fed in explicitly via `--exclude-from` (git treats `info/` as shared, so `--exclude-standard` alone would miss it). Entries are directories-first then files, each alphabetical.
- **Reading** (`ReadWorktreeFile`): refuses any path not surfaced by the listing (so `.git` and ignored files, e.g. `.env`, cannot be read), applies traversal rejection (`..`/absolute) plus canonicalize-and-contain under the worktree root, and caps content at **`MAX_WORKTREE_FILE_BYTES`** (1 MiB) with a `truncated` flag and full `byte_size`.
- **Tests**: unit coverage in **`worktree_files`** (`#[cfg(test)]`); integration coverage in **`worktree_files_rpc`**.

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

Project rows in **`projects.yaml`** may include optional **`main_branch_ref`** (`origin/<path>`, any remote branch — validated with **`validate_chain_pr_integration_base_ref`**, so multi-segment names like `origin/release/2025` are allowed). **`effective_integration_base_ref_for_project`** in **`project_storage`** returns a stored ref verbatim, or — for a legacy row with no stored ref — resolves the default **live** against the repository via **`resolve_default_integration_base_ref`** (`origin/master` → `origin/main` → `origin/HEAD`); the live probe is legacy-only and loses effect once a default is stored. **`set_project_default_branch`** updates the stored ref (validated before any write); invalid values fail it (and **`add_project`**) before the file is written. See [git-integration-base-ref.md](../../../../docs/ft/coder/git-integration-base-ref.md) and [project-concept.md](../../../../docs/ft/daemon/project-concept.md).

## Multi-host projects

A "host" is a daemon instance; the selectable set is the connected `tddy-daemon` LiveKit participants (`ListEligibleDaemons`, backed by common-room discovery). Discovery classifies each common-room participant and lists **only genuine daemons** — mirroring the web UI's `inferParticipantRole`, a participant with a browser identity (`web-`/`browser-`) or a coder/session identity (`server…`, `daemon-<uuid>`) is excluded even if it publishes advertisement-shaped metadata, and a daemon must publish a valid advertisement (no identity fallback). This keeps coder/session participants out of host selection and out of project fan-out (only daemons own projects). The same logical project can live on several hosts under **one shared `project_id`**:

- **Registry is per-daemon-per-user.** Each host keeps its own `~/.tddy/projects/projects.yaml`; a project "on" multiple hosts is one row per host, all sharing the `project_id`. Aggregated `ListProjects` returns one `ProjectEntry` per (`project_id`, hosting `daemon_instance_id`).
- **Adding to a host** (`AddProjectToHost`) routes to the target daemon (local, or forwarded over LiveKit) which clones the repo and writes a row reusing the `project_id`. **`project_storage::add_or_get_project`** makes this idempotent (append only when the id is absent; otherwise return the existing row).
- **Cross-host visibility** relies on `peer_project_entries` fanning out to peers' `ListProjects` with **`local_only = true`** — the flag is what stops a fanned-out call from recursing back into peers. `EligibleDaemonSource::peer_project_entries` is an `async` trait method (`#[async_trait]`), so the `ListProjects` handler awaits the fan-out directly on its runtime — no worker thread is parked and no multi-threaded runtime is required. `aggregate_peer_project_entries` fans out to all peers **concurrently** (`join_all`), so the aggregate is bounded by the slowest responsive peer (or the per-peer `PEER_PROJECT_FANOUT_TIMEOUT`), not the serial sum across peers; a peer that errors or times out contributes no rows.
- `ProjectData.host_repo_paths` / `project_storage::main_repo_path_for_host` resolve the per-host checkout path for a shared `project_id`.
- **Each daemon advertises its base clone location** (`repos_base_path`) on the `DaemonAdvertisement` published to the common room (`livekit_peer_discovery.rs`, populated from `config.repos_base_path_or_default()`, parsed back by `parse_daemon_advertisement_json`). The web reads the same `repos_base_path` JSON key into `DaemonHost.reposBasePath`.

### Auto-provisioning on session start

`StartSession` no longer requires the project to already be checked out on the target host. Once the peer route resolves to **local**, `ensure_project_available_for_start` runs `project_provision::ensure_project_available_locally` before dispatching by session type (so all four start paths — tool, claude-cli, sandboxed claude-cli, workspace — provision uniformly):

- Registered + checkout on disk ⇒ used as-is (no clone, no fan-out).
- Registered but checkout missing ⇒ re-cloned from the stored `git_url`.
- Not registered locally ⇒ peer-discovers `(name, git_url)` via `EligibleDaemonSource::peer_project_entries` (the same fan-out as aggregated `ListProjects`), clones into `repos_base_for_user(os_user, repos_base_path)/<name>`, and registers it via `add_or_get_project` (reusing the logical `project_id`).
- Unknown locally and on every peer ⇒ `NotFound`.

The helper takes injected `cloner` + `peer_lookup` closures for testability; the RPC wires the real cloner (`SpawnClient::clone_repo` when a spawn worker is configured, else `spawner::clone_as_user`, mirroring `AddProjectToHost`) and runs the blocking clone via `spawn_blocking` + `timeout`. Clone failures surface as errors (no masking); `NotFound` is preserved (not flattened to `internal`).

See [projects-screen-multi-host.md](../../../docs/ft/web/projects-screen-multi-host.md).

## Claude Code CLI sessions

When `StartSessionRequest.session_type == "claude-cli"`, the standard tool-spawn path is bypassed. Instead:

1. A dedicated git worktree is created under `{sessions_base}/worktrees/claude-cli-{short_id}/` (branch `claude-cli/{short_id}`).
2. `.session.yaml` is written with `session_type = "claude-cli"` and `model = <requested model>`. `repo_path` is set to the worktree path for later cleanup.
3. `ClaudeCliSessionManager::start()` spawns the `claude` binary (resolved from `claude_cli.binary_path` in config, default `"claude"`) with `--model <model> --session-id <session_id>` in the worktree directory.
4. `StartSessionResponse` returns empty LiveKit fields (`livekit_room`, `livekit_url`, `livekit_server_identity` all empty); the web client detects this and routes to `ConnectedClaudeCliTerminal`.

**`ClaudeCliSessionManager`** (`cli_session_manager.rs`, re-exported as `claude_cli_session`): in-memory registry (`HashMap<String, Arc<PtyHandle>>`) mapping session id to the active child process. `PtyHandle` holds:
- `stdin_tx`: `mpsc::UnboundedSender<Bytes>` — feed bytes into the child stdin
- `stdout_tx`: `broadcast::Sender<Bytes>` — subscribe for stdout/stderr chunks
- `worktree_path`: for deletion
- `pid`: for signal delivery

A background task monitors the child with `child.wait()`; on exit the entry is removed from the registry. `resume()` calls `start()` in the same worktree — the worktree's file state is preserved.

**`StreamSessionTerminalIO`**: The bidi gRPC stream calls `ClaudeCliSessionManager::get(session_id)` to look up the live `PtyHandle`. A write task reads `SessionTerminalInput` messages from the client stream and sends bytes to `stdin_tx`. A read task subscribes to `stdout_tx` and forwards chunks as `SessionTerminalOutput` to the client stream. Resize sequences (`\x1b]resize;...`) are intercepted before forwarding (no actual pty resize is performed; the sequence is dropped). Auth: `session_token` validated on the first message via the same GitHub → OS user path as other RPCs.

**`DeleteSession` for claude-cli**: After PID termination (SIGTERM / SIGKILL), the daemon also calls `remove_dir_all` on the worktree path stored in `metadata.repo_path`. The session directory is then removed as usual.

**`ReportSessionStatus`**: Hook-driven RPC. `tddy-tools session-hook` calls this after mapping a Claude Code or Cursor Agent CLI lifecycle event to a `SessionActivityStatus`. The handler validates `session_id` (path traversal guard), resolves `sessions_base` from `os_user` directly (no web-token path), reads `.session.yaml`, requires `session_type == "claude-cli"` or `"cursor-cli"`, constant-time-compares `hook_token`, then calls `update_activity_status(session_dir, status)`. See [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md#session-activity-status-via-per-worktree-hooks) and [cursor-cli-session.md](../../../docs/ft/daemon/cursor-cli-session.md#activity-status-hooks-cursorhooksjson) for hook flows.

## Cursor Agent CLI sessions

When `StartSessionRequest.session_type == "cursor-cli"`, the standard tool-spawn path is bypassed. The flow mirrors [Claude Code CLI sessions](#claude-code-cli-sessions) with Cursor-specific hooks and argv:

1. A dedicated git worktree is created (branch `cursor-cli/{short_id}` when `new_branch_name` is empty).
2. `<worktree>/.cursor/hooks.json` is written via `tddy_core::build_cursor_hooks_settings` (`install_cursor_hooks_in_worktree` in `cursor_cli_spawn.rs`). Each hook invokes `tddy-tools session-hook`, which reads Cursor's stdin JSON `hook_event_name` and calls `ReportSessionStatus`.
3. `.session.yaml` is written with `session_type = "cursor-cli"`, `model`, `repo_path`, `pid`, and `hook_token`.
4. `CliSessionManager::start_cursor()` spawns the Cursor Agent CLI binary (config `cursor_cli.binary_path`, default `"agent"`) with `--model <model>` and an optional initial prompt positional arg.
5. `StartSessionResponse` returns empty LiveKit fields; the web client routes to `ConnectedClaudeCliTerminal` (same gRPC terminal path as claude-cli).

**Sandbox:** `sandbox == true` with `session_type == "cursor-cli"` starts a sandboxed session via `start_sandboxed_cursor_cli_session` — see [Sandboxed Cursor Agent CLI sessions](#sandboxed-cursor-agent-cli-sessions). `managed_codebase`, `recipe`, and `specialized_agents` are honored on both sandboxed and non-sandboxed cursor-cli starts.

**`WaitingForInput`:** Cursor hooks do not emit a permission-prompt equivalent; `activity_status` never transitions to `WaitingForInput` for cursor-cli sessions.

**Config:** optional `cursor_cli:` block (`binary_path`, `tddy_tools_path`, `daemon_url`) — see `config::resolve_cursor_binary_path` and siblings.

**Telegram:** `/start-cursor <prompt>` — project → branch → `tcur:` model keyboard → spawn; see [telegram-session-control.md](../../../docs/ft/daemon/telegram-session-control.md#start-cursor-flow).

Feature reference: [cursor-cli-session.md](../../../docs/ft/daemon/cursor-cli-session.md).

## Managed-codebase workflow (workflow-aware claude-cli and cursor-cli)

When `StartSessionRequest.managed_codebase == true` **and** `recipe` is non-empty, a **claude-cli** or **cursor-cli** session (sandboxed or not) is launched *workflow-aware*: the agent drives and persists its own workflow state. The recipe is resolved once in the `StartSession` dispatch via `tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name` (an unknown name is `INVALID_ARGUMENT`) and threaded into both launch paths. See [managed-codebase-workflow.md](../../../docs/ft/coder/managed-codebase-workflow.md) for the product WHAT.

`ConnectionServiceImpl::prepare_managed_workflow` is the single helper shared by all start/resume sites (claude-cli and cursor-cli × non-sandboxed/sandboxed). It:

1. Builds the per-session wiring (`session_toolcall::set_up_managed_workflow` for a new session — controller at `recipe.start_goal()`; `resume_managed_workflow` for a resume — controller at the goal persisted in `changeset.yaml`, resolved by `managed_resume_goal`). The `WorkflowController` is registered as the **per-instance** `transition` handler of a per-session toolcall listener (`SessionToolcallListener`, bound on a short out-of-tree socket to satisfy `SUN_LEN`).
2. Writes the recipe's `orchestration_system_prompt` to `orchestration-prompt.txt` (the session dir for non-sandboxed; the jail-visible context dir for sandboxed) and returns it as `--append-system-prompt-file` for the **claude** argv. For **cursor-cli**, the same text is written to `<worktree>/.cursor/rules/tddy-managed-workflow.mdc` (Cursor has no `--append-system-prompt-file`).
3. Returns the per-session env (`TDDY_SOCKET` = the listener socket, plus a `PATH` that resolves `tddy-tools`).

For new sessions the changeset is seeded with `recipe.start_goal()` before `write_changeset`, and `recipe` is persisted in `.session.yaml` (the resume signal). The listener/`ManagedWorkflow` is owned by `ClaudeCliSessionManager` (non-sandboxed claude-cli/cursor-cli, dropped when the main terminal exits) or `SandboxSessionState` (sandboxed claude-cli or cursor-cli).

**How `transition` reaches the controller:** `tddy-tools transition` always runs **on the host** — sandboxed sessions relay the agent's `Shell` tool back to the daemon (`tool_engine::tool_shell`, which now runs the command with the per-session `extra_env`), and non-sandboxed sessions inherit the PTY env — so the child `tddy-tools` reads `TDDY_SOCKET` and dispatches over the existing relay to the per-session `ToolcallRpcService::with_transition_handler` in `tddy-core`. The process-global registry (`toolcall/transition.rs`) remains a fallback only for the in-process `tddy-coder`/`agent_session_runner` path; the per-instance handler prevents cross-session bleed under concurrency. Only `transition` is served on this listener (ask/approve use the MCP `approval_prompt` tool + PTY). Live workflow-event streaming to the web client is a follow-up; state is durable in `changeset.yaml`.

`prepare_managed_workflow` also exports **`TDDY_SESSION_DIR`** (the session dir) and **`TDDY_REPO_DIR`** (the worktree) in the per-session env, so a managed session's `tddy-tools` can locate the orchestrator changeset and run `git` against the repo — the PR-stack `pr_*` tools depend on both. (These come from the tddy-coder TUI backends on that path; the daemon-managed path must set them explicitly.)

## PR-stack child-spawn relay

A `pr-stack` orchestrator's agent spawns a planned node's child session from chat via the `pr_spawn_child` MCP tool, which relays a **`spawn-child`** toolcall verb over `TDDY_SOCKET` (request/response, returns the new `session_id`). It is served by a per-session **`StackChildSpawnHandler`** (`connection_service.rs`) bound to the toolcall listener **only** when the session's `managed_recipe.name() == "pr-stack"` (`with_child_spawn_handler` in `session_toolcall.rs`) — so an orchestrator can spawn children only for its own stack (the socket is per-session).

`spawn_child(node_id)` reads the orchestrator's `changeset.yaml` stack, finds the node (rejects an already-spawned node), derives the child's `initial_prompt` (title + description) and `new_branch_name` (node `branch`/`branch_suggestion`), inherits the orchestrator's `SessionMetadata.model` (errors if absent — no guess), then calls the shared **`spawn_claude_cli_session_inner`** with `stack_parent` = orchestrator, `managed_recipe = None`, `branch_worktree_intent = "new_branch_from_base"`. `spawn_claude_cli_session_inner` is the body of `start_claude_cli_session` extracted into a free async fn; the `StartSession` RPC method is now a thin wrapper over it, so the RPC path is unchanged. This is the same effect as the web "Start session" CTA, driven from the orchestrator chat.

> **Runtime verification pending:** the end-to-end spawn (orchestrator → `pr_spawn_child` → relay → `StackChildSpawnHandler` → child session materialized) has compile + unit coverage but no automated integration test; exercise it against a running daemon before relying on it.

## Sandboxed Claude Code CLI sessions

When `StartSessionRequest.session_type == "claude-cli"` **and** `sandbox == true` on macOS, the daemon uses the sandbox spawn path instead of `ClaudeCliSessionManager::start()`:

1. Creates the same git worktree as a non-sandbox claude-cli session (host `tool_engine::execute_tool` operates on this worktree).
2. Prepares a read-only **context dir** (`SandboxContextDir`: synced `CLAUDE.md`/`AGENTS.md`/skills + `REMOTE_APPENDIX`).
3. Renders an SBPL profile and spawns `tddy-tools sandbox-runner` via `sandbox-exec` (`tddy-sandbox-darwin`).
4. Waits for the ready marker, then **`dial_and_bridge`** dials the runner over its piped stdio (`--stdio`, via `bridge_sandbox_stdio` → `StdioSandboxClient`) for a single bidi **`SessionChannel`** (`sandbox_session.rs`) — no gRPC socket or port is involved for this call site (the runner's own tonic gRPC server is retained only for `tddy-sandbox-app`'s standalone demo path and `sandbox_action.rs`'s separate generic-action-execution flow).
5. Writes `.session.yaml` with `sandbox: true`; returns empty LiveKit fields.

**Specialized subagents:** the jail never mounts the repo — the agent reaches it only through the `mcp__tddy-tools__*` exec tools (this is what `managed_codebase` on `StartSessionRequest` names for users; it doesn't toggle mount behavior, since this path is always mount-free). When `specialized_agents` is non-empty, `ConnectionServiceImpl::specialized_subagent_env` resolves each name against the builtin + `<tddyhome>/agents` defs (an unresolvable name fails the request with `INVALID_ARGUMENT`) and adds `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` to the spawned jail's env, so the in-jail `tddy-tools --mcp` process registers the `subagent_new_session`/`subagent_prompt`/`subagent_cancel` MCP tools for those agents — see [specialized-subagents.md](../../../docs/ft/coder/specialized-subagents.md).

**Specialized-agent warm-up gate:** after resolving the defs and **before** spawning the jail, the start path calls `tddy_discovery::warmup::warm_up_agents` to wake each agent's model endpoint (`/v1/chat/completions` probe) and wait until it answers. A cold Ollama model or unreachable endpoint fails the start with `FAILED_PRECONDITION` (naming the agent, endpoint, and model) rather than stalling the main agent's first `subagent_prompt` — no fallback to starting the jail anyway. `502`/`5xx`/`429`/connection-errors retry until a 120s budget; a `404` fails fast. Resume reuses this start path, so it is gated too. Applies identically to the [cursor-cli sandboxed path](#sandboxed-cursor-agent-cli-sessions). See [specialized-subagents.md § Start-time warm-up gate](../../../docs/ft/coder/specialized-subagents.md#start-time-warm-up-gate).

**Claude binary + persistent jail `$HOME`:** the runner is always given the **real** `claude` as an absolute path. `config::resolve_claude_binary_path` prefers `~/.local/bin/claude`, then scans `$PATH` skipping wrapper-shim dirs (e.g. Superset's `~/.superset/bin`, which can't resolve inside the jail's `/usr/bin:/bin` PATH); overridable by the `TDDY_CLAUDE_BINARY` env var or an explicit `claude_cli.binary_path`. A bare name would give `binary_exec_reads` an empty parent (`Path::parent("claude") == Some("")`), emitting `(subpath "")` — which macOS `sandbox-exec` rejects (`empty subpath pattern`, exit 65) — so `binary_exec_reads` skips empty parents and `SandboxBuilder::build` drops empty-host reads (which would otherwise shadow the whole read allow-list). The jail `$HOME` is a **single daemon-wide persistent dir** (`config::resolve_claude_home_dir`: `TDDY_SANDBOX_CLAUDE_HOME` env > `claude_cli.claude_home_dir` > `$HOME/.tddy/sandbox-claude-home`), mounted read-write and reused across sessions so refreshed OAuth tokens, session history, and settings persist. `sandbox_session::prepare_persistent_claude_home` seeds `.claude/.credentials.json` once (non-clobbering) and mirrors the claude install so the in-jail self-check passes; the spawn passes `host_home: None` (`SandboxRunnerSpawn`) so the recipe's per-session credential copy can't overwrite a refreshed jail token. A shared **claude-sandbox config** file — per-OS `claude-sandbox.<os>.yaml` (`darwin`/`linux`) then generic `claude-sandbox.yaml`, or the `TDDY_SANDBOX_CONFIG` path (`config::resolve_sandbox_config_path`) — is intended to give the daemon parity with the `./claude-sandbox` launcher's `SandboxAppConfig`; **only the path resolver ships now — the loader that reads the file is a follow-up.**

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

## Sandboxed Cursor Agent CLI sessions

When `StartSessionRequest.session_type == "cursor-cli"` **and** `sandbox == true`, the daemon uses the sandbox spawn path (`start_sandboxed_cursor_cli_session`) instead of `CliSessionManager::start_cursor()`:

1. Creates the same git worktree as a non-sandbox cursor-cli session (host `tool_engine::execute_tool` operates on this worktree).
2. Prepares a read-only **context dir** (`SandboxContextDir`: synced project docs + managed-codebase appendix when applicable).
3. Renders an SBPL profile from `tddy-sandbox-recipes::cursor_cli` and spawns `tddy-sandbox-runner --agent-kind cursor` via Seatbelt (macOS) or cgroups+namespaces (Linux).
4. Waits for the ready marker, then **`dial_and_bridge`** dials the runner over piped stdio (`--stdio`) for a single bidi **`SessionChannel`** — same transport as sandboxed claude-cli.
5. Writes `$HOME/.cursor/mcp.json` in the jail (registers `tddy-tools --mcp`). Headless MCP approval flags are **not** auto-injected — callers pass `--approve-mcps` / `--force` / `--trust` via agent args when using print mode.
6. Spawns the Cursor Agent CLI via direct `node index.js` (bypasses the bash `agent` wrapper; `realpath` fails under Seatbelt).
7. Writes `.session.yaml` with `sandbox: true`; returns empty LiveKit fields.

**Specialized subagents** and **managed codebase** follow the same rules as [Sandboxed Claude Code CLI sessions](#sandboxed-claude-code-cli-sessions): the jail never mounts the repo; `specialized_subagent_env` and `prepare_managed_workflow` wire `TDDY_SUBAGENTS_JSON`, `TDDY_SOCKET`, and orchestration via `.cursor/rules/tddy-managed-workflow.mdc`.

**Terminal I/O**: `StreamSessionTerminalIO` / `StreamTerminalOutput` / `SendTerminalInput` delegate to `SandboxSessionManager` when `metadata.sandbox == true` (same as claude-cli).

**Lifecycle**:
- **`DeleteSession`**: stops the `SandboxHandle`, removes worktree and session dir.
- **`ResumeSession`**: non-sandbox cursor-cli resumes via `CliSessionManager`; **sandboxed cursor-cli resume relaunch is not yet implemented**.

**Standalone proof:** `tddy-sandbox-app --agent-kind cursor --repo <path> --cursor-binary <agent> [-- -p hi]` exercises the darwin Seatbelt path without the daemon.

Feature reference: [cursor-cli-session.md](../../../docs/ft/daemon/cursor-cli-session.md#sandbox-mode).

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

## Host stats (Host Stats Footer)

Two unary RPCs feed the web's bottom **Host Stats Footer** (see
[docs/ft/web/host-stats-footer.md](../../../../docs/ft/web/host-stats-footer.md)) with host-level
telemetry for the daemon the client is addressing:

- `GetHostCpuStats()` → `per_core_percent: []float` — utilization (0..100) of each logical core,
  core 0 first. The web polls it every 5 s.
- `GetHostDiskStats()` → `{available_bytes, total_bytes, project_dir}` for the filesystem holding
  the daemon's default project directory. The web polls it every 60 s.

Both authenticate `session_token` via the same GitHub → OS user path as the other endpoints, and are
addressed to the daemon participant directly (no `daemon_instance_id` payload — the LiveKit transport
already targets `daemon-{instanceId}`).

Backed by **`host_stats.rs`**: the `HostStats` trait — injected via
`ConnectionServiceImpl::with_host_stats` so tests substitute a deterministic fake — with a
`sysinfo`-backed `SysinfoHostStats`. CPU sampling holds a long-lived `sysinfo::System` (constructed
once with the service) so successive ~5 s-apart refreshes report real per-core deltas; the first
sample reads ~0. Disk resolution enumerates mounts and picks the filesystem whose mount point is the
longest **path-component** prefix of the project directory (`select_mount_for_path`), falling back to
the largest mount by capacity if none is a prefix. The default project directory resolves to
`$HOME/<repos_base_path_or_default>` (`DaemonConfig` has no explicit project-dir override today).

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

## Agent-activity log & stream

A **separate** per-session log — **`~/.tddy/sessions/{session_id}/agent-activity.jsonl`** — records the
**agent's own** tool loop (Read, Shell/Bash, Edit, `tddy-tools` verbs), as opposed to the
human-triggered `ExecuteTool` invocations captured in `tool-calls.jsonl`. The record shape
(`AgentActivityRecord`) and the append/coalesce/500-cap read logic live in **`tddy-core::agent_activity`**
so every host writes the same format; see [tddy-core architecture § Agent activity](../../tddy-core/docs/architecture.md#agent-activity-agent_activity).

- **Capture (one seam per session type):**
  - **sandbox** — the host-side executor `DaemonToolHandler::execute` appends a `running` row then a
    terminal `completed`/`error` row around `tool_engine::execute_tool_with_env`, publishing each to the hub.
  - **claude-cli** — the `PreToolUse`/`PostToolUse` hooks (`tddy-tools session-hook`) POST `ReportAgentActivity`;
    the daemon pairs Pre→Post per session.
  - **tool / cursor-cli** — served by the **coder participant** over LiveKit while the session is live
    (its presenter appends rows and broadcasts `PresenterEvent::AgentActivity`); the daemon serves the file
    snapshot over `/rpc` as fallback.
- **`AgentActivityHub`** — `Mutex<HashMap<sessionId, broadcast::Sender<AgentActivityRecord>>>`, one broadcast
  channel per session. `StreamSessionActivity` mirrors the snapshot-then-live `StreamTerminalOutput` /
  `WatchTerminalControl` pattern (snapshot via `read_agent_activity`, then relay hub events with `Lagged`
  handling); `ReportAgentActivity` and the sandbox executor are the publishers.
- **Cross-host limitation:** `StreamSessionActivity` serves Local routes only and rejects `PeerRoute::Forward`
  with `unimplemented` — a streaming peer-forward primitive is a tracked follow-up (`forward_to_peer` is
  unary-only). Single-host (the common case) works fully. Feature: [agent-activity-pane.md](../../../docs/ft/web/agent-activity-pane.md).

## See also

- **Worktrees**: **`ListWorktreesForProject`** (cached rows; **`refresh`** runs **`WorktreeStatsCache::refresh_stats_for_project`** in a blocking worker), **`RemoveWorktree`** ( **`remove_worktree_under_repo`**, then **`invalidate_project`**). Project checkout: **`main_repo_path_for_host`** with the local **`daemon_instance_id`**. Details: [worktrees.md](./worktrees.md), [docs/ft/web/worktrees.md](../../../../docs/ft/web/worktrees.md).
- Feature: [Session directory layout](../../../../docs/ft/coder/session-layout.md)
- Feature: [docs/ft/daemon/project-concept.md](../../../../docs/ft/daemon/project-concept.md)
- Feature: [Cursor Agent CLI session](../../../../docs/ft/daemon/cursor-cli-session.md)
- [changesets.md](./changesets.md)
