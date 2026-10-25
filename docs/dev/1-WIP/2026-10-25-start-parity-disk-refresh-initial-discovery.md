# Initial discovery — `#start-parity` 8/9 `disk-refresh`

Exploration 1–3 are the whole-work discovery, copied in full. Exploration 4 is this node's own.

## Combined conclusions

- **The form has one request builder.** `buildStartSessionRequest` in
  `packages/tddy-web/src/components/sessions/createSessionRequest.ts:69-229` reads
  `CreateSessionFormValues` (:28-62), which is a TS interface (no zod, no runtime key list). It has
  hand-written branches per session type. The form always sends via `streamStartSession`. The
  proto `StartSessionRequest` is the only shared source.
- **The form never sets these request fields:** `repo_path`, `claude_args`,
  `requested_session_id`, `agent_clone`, `split_agent`, `github_source`, `session_role`.
- **The unary `StartSession` is not a task, and cannot be cancelled.** It uses a discarding sink
  and a fresh token. Only the local `StreamStartSession` registers a `start_session` task.
- **A start task keeps nothing a poll could read.** Phases and clone progress go only into the
  stream's mpsc, and `result_json` is never set.
- **Of the `TaskService` calls, only `CancelTask` forwards to a peer.** `GetTask` and `ListTasks`
  are local only, and a terminal task is evicted 5 minutes after it ends.
- **The host roster server-side** is `HostService.ListEligibleDaemons`, the same source routing
  validates against. The web form uses a client-side LiveKit roster instead.
- **Recipes:** the server lists 10 (`supported_workflow_recipe_cli_names()`); the form hardcodes 8.
- **`spawn_conversation` is a separate path.** It is grill-me only, claude-cli only and local only,
  and calls `spawn_claude_cli_session_inner` directly.

## Exploration 1 — New-session form inventory

Paths are under `packages/tddy-web/src/`. CSR = `components/sessions/createSessionRequest.ts`.

**Builder**
- `commonParams` (CSR:104-120). Per-type branches:
  - `tool`: 121-150
  - `cursor-cli`: 151-173
  - `opencode-cli`, `muse-cli`, `warp-cli`: 174-204
  - `claude-cli`: 205-228
- `claudeCliAgentChoices` (236-254).
- The project-home variant `buildProjectHomeStartSpec` (261) goes to `EnsureProjectHomeSession.start`.
- The form object is assembled in `CreateSessionPane.tsx:249-281`.
- Submit is in `useCreateSessionSubmit.ts:80-132`: the attachments are staged first, then the
  form calls `streamStartSession` (`hooks/useSessionAttachments.ts:391-397`).
- Branch-conflict resubmit: `lib/branchConflict.ts:43-64`.

**Fields the form sets, and where their options come from**

| Field | Sent for (CSR line) | Options / control |
|---|---|---|
| `project_id` | all | `ProjectService.ListProjects` |
| `branch_worktree_intent` | all | static: `new_branch_from_base`, `work_on_selected_branch` |
| `new_branch_name` | all | free text |
| `create_remote_branch` | all | bool, shown when the daemon cuts the worktree |
| `selected_integration_base_ref` | all | `initialValues.baseBranchOptions` (PR-stack screen only) |
| `selected_branch_to_work_on` | all | `ProjectService.ListProjectBranches{projectId, daemonInstanceId}` |
| `daemon_instance_id` | all | `useDaemons()` → `directory.hosts`, a client-side roster |
| `on_branch_conflict` | all, hardcoded `"reject"` | — |
| `attachments` | all | staged upload or `hostDocument` |
| `tool_path` | `tool` | `CatalogService.ListTools[0]`; no control |
| `agent` | `tool` | `CatalogService.ListAgents`, fanned out to all hosts |
| `recipe` | `tool`; `cursor-cli` and `claude-cli` when managed (not split) | static `WORKFLOW_RECIPES` (8), `createSessionRecipes.ts:2-11` |
| `pr_stack_base_session_id` | `tool` with recipe `pr-stack` | ListSessions filtered (`utils/stackParents.ts:91-102`) |
| `stack_parent`, `stack_node_id`, `stack_parent_daemon_instance_id` | all except main | ListSessions filtered (`stackParents.ts:30-45`) |
| `session_type` | all (`tool` sends `""`) | static `SESSION_TYPE_OPTIONS` (`cliHarnessOptions.ts:4-11`) |
| `model` | all; `warp-cli` forced `""` | `CatalogService.ListAgentModels{agent, daemonInstanceId}`, or `AccountsService.ListProviderAccountModels` |
| `provider_account_id`, `provider_built_in` | `tool`, opencode/muse/warp, `claude-cli` (**not** `cursor-cli`) | `AccountsService.ListProviderRoutes{harness}` |
| `permission_mode` | `claude-cli` | static five, inline JSX (`CreateSessionPermissionFields.tsx:53-57`) |
| `dangerously_skip_permissions` | `claude-cli`; forced false on a split or sandboxed codebase | bool |
| `initial_prompt` | `cursor-cli`, `claude-cli` | text |
| `sandbox`, `managed_codebase` | `cursor-cli`, `claude-cli` | bool |
| `sandboxed_codebase` | `claude-cli` | bool; gated on the host's `DaemonHost.sandboxedCodebase` |
| `codebase_daemon_instance_id` | `claude-cli` when split | hosts, plus "Same as host" |
| `ssh_config_host` | `claude-cli` when managed | `HostService.ListSshConfigHosts{daemonInstanceId}` |
| `specialized_agents`, `semantic_index` | `cursor-cli` and `claude-cli` (managed) | `CatalogService.ListSubagents`, fanned out |

**Client-side rules**
- Create needs:
  - `tool`: project, agent, tool path and model;
  - `warp-cli`: project;
  - any other CLI type: project and model.
- Sandboxed codebase clears sandbox, managed and the codebase host; sandbox or managed clears the
  sandboxed codebase.
- Turning managed off clears `semantic_index` and `specialized_agents`.
- The recipe is dropped when the codebase is split or managed is off.

## Exploration 2 — start internals, forwarding, task, lists, spawn_conversation

**Serving**
- `tddy-session-lifecycle/src/service.rs:40,47`
- → `connection_service/svc_session_lifecycle_ports.rs:28,37-59`
- → `tddy-agent-launch/src/session_coordinate_handlers.rs`:
  - unary: `start_session_at_session_coordinate`, :93-105
  - stream: `stream_start_session_at_session_coordinate`, :293-358

**Unary start**
- `start_session_core` (`harness_profile_request.rs:29`, fresh `CancellationToken`)
- → `start_session_on_request` (`svc_start_session_core.rs:64`)
- Routing (:100-122): `eligible_daemon_ids()` (`start_request_checks.rs:96`), then
  `classify_peer_route` (`tddy-daemon-kernel/src/peer_forwarding.rs:35`).
- A forward goes through `forward_start_naming_local_source` (:144) and `forward_start_session`
  (:109-140).
- That calls `peer_calls::forward_start_session_relaying` (`tddy-daemon-kernel/src/peer_calls.rs:105-133`),
  which opens a `StreamStartSession` on the peer, discards the events, and returns the result.
  It needs the common room.

**Streamed start**
- Forward: `peer_calls::forward_stream_start_session` (:56-69). The peer's `StartTaskStarted` is
  relayed through.
- Local: `task_registry.spawn(StartTask{…}, START_TASK_KIND="start_session", "", vec![])`
  (`session_coordinate_handlers.rs:344-356`, `start_task.rs:32`).
- `StartTask::run` (`start_task.rs:46-82`): events go only to the mpsc. The outcome sets the task
  status: Ok → Completed, Cancelled → Cancelled, Err → Failed(message). `set_result` is never
  called.
- A cancel after a step that cannot be interrupted runs `undo_a_cancelled_start` (:98-125).

**`project_home.rs` start**
- `DaemonProjectHome::start_session` (:297-328) calls the unary start with `daemon_instance_id`
  empty.
- `tag_orchestrator` (:381) writes the local changeset after the start.

**TaskService** (`tddy-daemon-sandbox/src/task_service.rs`)
- Mounted in `tddy-daemon/src/runtime/mount_entries.rs:168-186`.
- `list_tasks` (:132) and `get_task` (:144) are local only.
- `cancel_task` (:269-296) forwards via `peer_calls::forward_cancel_task` (:142-155).
- `watch_task` and `watch_task_list` refuse a remote target.
- Registry: `TERMINAL_TASK_CAP=200`, `TERMINAL_TASK_TTL=5min` (`tddy-task/src/registry.rs:12-14`).
- After eviction a GetTask answers NOT_FOUND.

**Lists**
- **Hosts:** `ListEligibleDaemons` (`tddy-host-service/src/service/listings.rs:25-49`, trait
  `multi_host.rs:24`, LiveKit impl `livekit_peer_discovery.rs:679`). `ListKnownHosts` (listings.rs:53).
- **Agents:** `CatalogService.ListAgents` → `listed_agents` (`tddy-daemon-rpc/src/catalog/agent_rows.rs:17-33`).
- **Models:** `ListAgentModels` (`catalog/ports.rs:72`) shells out to `tddy-tools list-models`, is
  cached, and is local only.
- **Tools:** `ListTools` (:26).
- **Subagents:** `ListSubagents` (:167).
- **Recipes:** `tddy_workflow_recipes::approval_policy::supported_workflow_recipe_cli_names()`
  (`approval_policy.rs:5`), 10 names.

**`spawn_conversation`**
- MCP tool: `tddy-tools/src/server.rs:1001-1023`, input struct :260, request builder :277.
- CLI: `tddy-tools/src/cli.rs:57,353-385`.
- Listener: `tddy-toolcall/src/toolcall/listener.rs:439`.
- Relay: `tddy-coder/src/conversation_spawn_relay.rs:157` → `tddy-host-service/src/host_session_service.rs:193`.
- Handler: `tddy-agent-launch/src/conversation_spawn_handler.rs:13-97`, enabled only for recipe
  `grill-me` (`conversation_spawn.rs:15`).
- Bound at `svc_start_claude_cli_session.rs:171-187,225-240`.
- The handler calls `spawn_claude_cli_session_inner` (`claude_cli_spawn.rs:39`) directly, with a
  discarding sink.

**Parity anchor:** `tddy_service::SERVICE_DESCRIPTOR_BYTES` (`packages/tddy-service/src/lib.rs:449`)
is the compiled-in `FileDescriptorSet`, and it is decoded in the existing tests
(`integration_tests.rs:1581`).

## Exploration 3 — Worktrees screen, disk refresh, Terminate, activity and transcript, forwarding

**How the web reaches a host.** The web addresses the owning daemon directly over LiveKit
(`daemon-{id}`, through `useDaemonClientFor`). It almost never relies on `daemon_instance_id`
forwarding.

**Worktrees screen (#730)**
- Files: `tddy-web/src/components/worktrees/WorktreesAppPage.tsx:33-150`, `WorktreesScreen.tsx:33-141`,
  `lib/worktreeList.ts`, `rpc/useWorktreeStatsStream.ts`.
- RPC: `WorktreeService.StreamWorktreeStats{session_token, project_id, recalculate_all}`.
  - It returns a `WorktreeStatsEvent{snapshot[] | updated}` of `WorktreeRow` (`worktree.proto:155-167`):
    `path`, `branch_label`, `disk_bytes`, `changed_files`, `lines_added`, `lines_removed`,
    `updated_at_unix_ms`, `stale`, `size_status` (NONE/CALCULATING/CACHED), `size_calculated_at_unix_ms`.
  - `ListWorktreesForProject` is not called by the web.
- Sort: `sortWorktreesBySize` (`worktreeList.ts:34-44`) — largest `disk_bytes` first, unsized last,
  name breaks ties. There is no paging.
- Detail view: `sessions/SessionWorktreeTab.tsx:31-208`. It shows disk ("Calculating…" until
  cached), last calculated, branch and changed files. Its actions are Refresh
  (`CalculateWorktreeSize`), Clear (`CleanWorktree`), Delete (`RemoveWorktree`) and, inside a
  session, Restore.
- Server: `tddy-worktree-service/src/service.rs`.
  - Handlers: `stream_worktree_stats` :630, `calculate_worktree_size` :733, `list_worktrees_for_project`
    :335, `remove_worktree` :389, `clean_worktree` :431.
  - **No peer forwarding; no `daemon_instance_id` on any worktree request** (:1-7).

**Disk refresh**
- The screen's Refresh re-subscribes with `recalculate_all: true`. The per-worktree Refresh calls
  the unary `CalculateWorktreeSize`, which returns `{ok, message}` at once.
- The calculator is `WorktreeSizeCalculator` (`worktrees.rs:424-600`).
  - It sizes in the background: `enqueue`, then `tokio::spawn`, with a semaphore of 2 and the walk
    in `spawn_blocking`.
  - State per path: NONE / CALCULATING / CACHED, persisted to
    `{data}/projects/{project}/worktree_sizes.json`.
  - `state()` reads the persisted file when it has nothing in memory.
- The UI's progress is "Calculating disk usage… {done} of {total}", counted from row statuses.
  **No ledger is needed.**

**Terminate**
- `useSessionsDrawerPaneActions.ts:114-129` calls `SessionService.SignalSession{session_token,
  session_id, signal: SIGTERM}` on the owning host.
- The inspector's Terminate has no confirmation. The terminal chrome's menu confirms with "This
  will stop the remote session and end the process on the host. Continue?"
- Handler: `tddy-agent-launch/src/svc_signal_delete_session.rs:26-114`. It reads the PID, returns
  FAILED_PRECONDITION if the process is dead, and otherwise sends `kill(pid, SIGTERM)`.
- **It does not touch the session dir, the worktree or the branch.** There is no peer routing.
- `DeleteSession` (:116-195) is a different action: it removes the worktree and the session dir,
  and never the branch.

**Activity and transcript**
- The UI's Activities view (`SessionActivitiesPane.tsx`, `useAcpReplay.ts`) uses three RPCs:
  - `StreamAcpReplay` (count, or tail then live, `page_size` 100);
  - `GetAcpReplayPage{before_seq, page_size}` → `{frames[], first_seq, at_oldest}`;
  - `GetAcpToolCallDetail{tool_call_id}` → `{raw_input?, raw_output?}`.

  All are in `activity.proto`.
- `GetAcpReplayPage` and `GetAcpToolCallDetail` forward to a peer through `PeerRoutedActivity`
  (`svc_activity_ports.rs:275,297`). The streams refuse a peer target.
- Source: `acp_replay.rs:99-110` `read_session_transcript` merges `acp-transcript.jsonl` (coder
  presenter only) with `agent-activity.jsonl`. **A claude-cli session's history therefore holds
  only tool calls.**
- `agent-activity.jsonl` lives in `tddy-changeset/src/agent_activity.rs`.
  - `AgentActivityRecord` fields: call_id, tool_name, input, status, result, error_message,
    started/completed ms, source, head_commit, activity_seq, changed_paths.
  - `read_agent_activity` keeps the newest 500 calls.
- **Pending question.**
  - The replay hard-codes `pendingQuestion: null`.
  - The live chat captures it from the ACP `requestPermission`: `title` = question,
    `raw_input` = header, the options, a `:multi` suffix for multi-select, and `other` for free
    text (`useAcpSession.ts:381-406`).
  - The server keeps only the bool `SessionMetadata.pending_elicitation`
    (`tddy-changeset/src/session_metadata.rs:28`), set by tool sessions and left empty for
    claude-cli (`hooks_and_urls.rs:72`). **The question text is not persisted.**

**Forwarding helpers**
- `ExecuteTool` already forwards a whole tool call on `ExecuteToolRequest.daemon_instance_id`
  (`tddy-daemon-rpc/src/exec_tool/ports.rs:42-53`, through
  `PeerRouting::rpc_served_by_peer`, `tddy-host-service/src/peer_routing.rs:166-189`).
- A project-home tool forwarded that way is refused on the peer: `project_home_caller` reads the
  home session's record locally, and that record is not there.
- `peer_calls.rs` has `forward_delete_session`, but no signal, worktree or activity forwards.
  `forward_unary` is private.

## Exploration 4 — `disk-refresh`

- The screen's Refresh re-subscribes `StreamWorktreeStats{recalculate_all: true}`. The
  per-worktree Refresh calls `CalculateWorktreeSize`, which returns at once.
- `WorktreeSizeCalculator` (`tddy-worktree-service/src/worktrees.rs:424-600`) sizes in the
  background (two walks at a time), keeps NONE/CALCULATING/CACHED per path, and persists the
  results.
