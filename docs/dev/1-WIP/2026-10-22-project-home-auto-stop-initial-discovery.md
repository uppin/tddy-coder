# Initial Discovery: Project-home auto-stop (#project-home 6/7)

**Changeset**: [2026-10-22-project-home-auto-stop.md](./2026-10-22-project-home-auto-stop.md)
**Date**: 2026-10-22
**Passes**: 3

## Combined Conclusions

**This node:** #project-home 6/7 — see the changeset's `## Responsibility`. The passes below are the stack-wide discovery, copied in full (Exploration 1–3); node-specific passes are appended after them.

Stack `#project-home`, 7 nodes (approved 2026-10-22). Decisions from the interview: home agent has **no direct edits** (no write/shell tools; state changes only through daemon tools); **idle auto-stop** N minutes after the last turn; activity sidebar default-open for **every managed session plus the home session**; project-less sessions go in a trailing **"Other"** group; home session lives in a **dedicated home worktree** (detached at `main_branch_ref`, fast-forwarded on resume); **rebase is gated** with stash/reset/checkout/clean (refused while the target worktree's session is active); commit/push always allowed; home session **starts on click** of its pinned entry.

State A, by area:

- **Web drawer** (`components/sessions/SessionDrawer.tsx`, `utils/sessionStackGroups.ts`): flat `SessionEntry[]` → active/remaining partition (`connectionStatusForSession`) → stack groups by `orchestratorSessionId`. Sessions already carry `projectId`; project names come from `ListProjects` (`useSessionsDrawerProjects.ts`). No per-project grouping.
- **Activity**: the Agent Activity transcript is a top-bar popover (`AgentActivityOverlay`, `SessionMainPane.tsx:279`) over `useAcpReplay`/`StreamAcpReplay`; dormant sessions use `SessionActivitiesPane` as base view. No right sidebar. `SessionMainPane.tsx` is 674/500 lines (unclaimed code issue) → new code in new modules.
- **Managed flag**: `managed_codebase` exists only on `StartSessionRequest`/placement; it is not persisted (`SessionMetadata`), not listed (`session_reader`), not on proto `SessionEntry`; the web cannot tell a managed session.
- **Projects**: `ProjectData` in `projects.yaml` (`tddy-projects/src/project_storage.rs`), per-field setters (`SetProjectDefaultBranch`, `SetProjectAccounts`) through `handler.rs`/`service.rs`/`lib.rs` and `tddy-daemon-rpc/src/project/coordinate_handlers.rs` (650/500, unclaimed). Setters reach one host (todo 2026-10-14). Agent choices = `CatalogService.ListAgents` (`AgentInfo{id,label}`, includes harness profiles).
- **Lifecycle**: `ResumeSession` already continues the agent conversation for claude-cli, cursor-cli (`cursor_chat_id`), other CLI harnesses and tool sessions. No per-session idle stop exists; `tddy_task::IdleTimeoutTracker` is the reusable primitive. Managed sessions run the agent in a read-only context dir, every code op through `ExecuteTool` against a worktree; claude-cli enforced via `build_claude_allowlist` + `--disallowedTools`; cursor-cli cannot be enforced (todo 2026-08-13).
- **Agent tools** (`tddy-tools/src/server.rs`, 3896 lines, unclaimed oversized issue): `spawn_conversation`, `pr_*`, `subagent_*`. No session list/status, no cross-session worktree read, no general commit/push/rebase/reset on a target worktree. `subagent_pull` is the only "refuse while a turn runs" gate. `tddy-worktree-service/src/branch_owner.rs` holds the "active claimant wins" rule over a `SessionListing` port — the basis for the destructive-git gate.
- **Code issues in the path**: none claimed by another PR. Oversized/complex (unclaimed): web `SessionMainPane.tsx`, tools `server.rs`, tool-engine `lib.rs`, daemon-rpc `project/coordinate_handlers.rs`, session-lifecycle `connection_service.rs`, agent-launch `svc_start_session_core`.

Open questions for green: whether the home worktree is a `workspace`-type session (no PTY) paired with a claude-cli agent, or one managed claude-cli session whose worktree is the home worktree — settle in node 4's red step.

## Exploration 1: agent tool surface — sessions, worktrees, git — 2026-10-22

**Agent**: Explore subagent
**Scope**: what tools a running agent has to reach the daemon; session create/list/status, worktree access, git ops; read-only enforcement

### Sequence

1. `ls packages`, `packages/tddy-tools/src`, `docs/dev/todo` — map workspace, tool modules, backlog.
2. Grep `DaemonUds\|enum SessionToolTransport` over `packages/*.rs` — transport enum and users.
3. `wc -l` tddy-tools sources; `ls` of `tddy-session-tool-client`, `tddy-worktree-service`, `tddy-session-actions`, `tddy-session-agents`, `tddy-pr-stack`, `tddy-session-worktree`.
4. Read `packages/tddy-tools/src/cli.rs` (full) — CLI subcommands.
5. Read `packages/tddy-tools/src/cli/session_tools.rs` (full) — `list-tools` / `call-tool`.
6. Read `packages/tddy-session-tool-client/src/lib.rs` (full) — transport detection + dispatch.
7. Grep `server.rs` for `#[tool(`, `env::var`, `async fn`.
8. Read `server.rs:1-720` — `PermissionServer`, advertisement, permission decision.
9. Read `server.rs:715-1044` — static `#[tool]` definitions.
10. Read `server.rs:1480-1779` — MCP handler, dynamic exec-tool router, `dispatch_dynamic_tool`.
11. Read `server.rs:2658-2830` (subagent router), `subagent_diff.rs`, `subagent_pull.rs:1-40`, `subagent_end.rs:1-40`.
12. `ls tddy-tools-cli/src`, `tddy-tool-engine/src`; read `tddy-tools/src/lib.rs`; grep subcommand match in `tddy-tools-cli/src/main.rs`.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `DaemonUds` | `packages/` | `tddy-session-tool-client/src/lib.rs:40,55,276,362`, `src/conversation.rs:59,150`, `tddy-discovery/src/roster/stream.rs:274,279`, `tddy-discovery/src/roster/link.rs:66` |
| ls | crates | `packages/` | `tddy-worktree-service/src/{branch_owner,base_sync_cache,worktree_files,worktrees,service,remote_git_service}.rs`; `tddy-session-worktree/src/{base_sync,session_chain}.rs`; `tddy-pr-stack/src/{git_ops,rpc}.rs`, `stack_ops`; `tddy-session-agents/src/{svc_resolve_listed_worktree,session_agent_status}.rs`; also `tddy-session-catalog`, `tddy-session-lifecycle`, `tddy-projects`, `tddy-cli-sessions` |

### Inspected files

#### `packages/tddy-tools/src/cli.rs`
- `spawn_conversation` CLI (55-62, 353-386): `--data '{"prompt","branch","base_ref"}'`, sends `{"type":"spawn-conversation",...}` via `dispatch_toolcall` over `TDDY_SOCKET`.
- `submit` (179), `ask` (318), `transition` (390), `list-actions`/`invoke-action` (412-440) relay over `TDDY_SOCKET`.

#### `packages/tddy-tools/src/cli/session_tools.rs`
- `CLI_SUBCOMMAND_TOOLS` (7-28): submit, ask, transition, get-schema, build. `all_session_tools()` (42) = `PermissionServer::advertised_tool_defs()` + CLI tools. `call-tool` (73-174) dispatches to `PermissionServer::call_tool_by_name`.

#### `packages/tddy-session-tool-client/src/lib.rs`
- `SessionToolTransport` (40): `SandboxIpc` (`TDDY_SANDBOX_TOOL_IPC`), `DaemonUds { socket_path, session_id, session_token, daemon_instance_id }` (55; when `TDDY_REMOTE_SESSION_ID` + `TDDY_REMOTE_DAEMON_SOCKET`, 272-282), `DaemonHttp` (`TDDY_REMOTE_DAEMON_URL`, Connect POST `ExecToolService/ExecuteTool`), `LiveKit`, `IncompleteLiveKit`. Detection `detect_session_tool_transport()` (234). `dispatch_session_tool(tool_name, args)` (354) = single entry into daemon `ExecuteTool`.
- Re-exports `conversation_worktree`, `diff_conversation_worktree`, `pull_conversation_range`, `reset_conversation_worktree`, `sync_conversation_worktree` (22-26).

#### `packages/tddy-tools/src/server.rs` (3896 lines)
- `PermissionServer::new()` (304-358) layers: static `#[tool]` set; exec-tool catalog (`tddy_tool_engine::tool_catalog()`) only when a session-tool transport is detected, minus subagent `replaces`; action tools gated by `TDDY_SESSION_ACTION_TOOLS`; subagent tools (advertised while roster has an agent); `Lsp*` by `TDDY_LSP_TOOLS`; restructure by `TDDY_RESTRUCTURE_TOOLS`.
- `advertised_tools()` (389), `call_tool_by_name` (453-501).
- `decide()` (602-641) — permission policy behind `approval_prompt`: always allow Bash starting `tddy-tools` and `mcp__tddy-tools__*`; pre-allow Bash/Write/Edit/Read/Glob/Grep when every path under `TDDY_SESSION_DIR` or `TDDY_REPO_DIR`; always allow `ExitPlanMode`/`EnterPlanMode`/`AskUserQuestion`; else `{"type":"approve"}` over `TDDY_SOCKET` (60s), denied without a socket.
- Static tools (717-1007): `approval_prompt`, `github_create_pull_request`, `github_update_pull_request`, `pr_stack_status`, `pr_merge`, `pr_close`, `pr_repoint`, `pr_resolve_conflicts` (detect-only; `worktree_dir`,`base_ref`), `pr_set_status`, `pr_add_planned`, `pr_update_planned`, `pr_delete_planned`, `pr_set_parents` (rebase + force-push-with-lease + retarget), `pr_read`, `pr_search`, `pr_comments`, `pr_adopt`, `pr_spawn_child` (relay `spawn-child`), `spawn_conversation` (relay `spawn-conversation`, new worktree, this session as orchestrator).
- `dispatch_dynamic_tool` (1589): refuses withdrawn tools then `dispatch_session_tool` — server half of two-layer enforcement; `--allowedTools` is spawn half.
- Subagent router (2658-2830): `subagent_new_session`, `_prompt`, `_resume`, `_await`, `_cancel`, `_end` (applies commits, deletes conv worktree), `_pull` (3-way apply; **refused while a turn is running**), `_diff` (read-only, allowed during a turn), `_list`, `_status` (`idle|running|executing_tool|waiting_for_input|connecting|error|unknown`).

#### `packages/tddy-tools-cli/src/main.rs:164-187`
Subcommands: Submit, Ask, SpawnConversation, ListTools, CallTool, Transition, GetSchema, ListSchemas, SetSessionContext, PersistChangesetWorkflow, ListActions, InvokeAction, BuildList, Build, PtyRelay, Remote, SessionHook, ListModels, Analyze, Restructure, Extensions.

### Findings

1. Creating sessions: `spawn_conversation` / `pr_spawn_child` relay over `TDDY_SOCKET` to the session host. A `create_session` tool would follow the same path.
2. **No tool lists sessions or their status** beyond this session's own roster (`subagent_*`) and stack nodes.
3. **No tool reads/diffs another session's worktree**; `subagent_diff` is the model (read-only, via daemon).
4. Git actions today: `pr_set_parents`, `pr_merge`/`pr_close`, `pr_repoint`, `pr_resolve_conflicts` (detect), `subagent_pull`/`_end`. No general commit/push/rebase on an arbitrary worktree.
5. Only "is active" gate found: `subagent_pull`/`_end` refused while a turn runs.
6. Read-only today: `decide()` pre-allows writes inside repo/session dir — **not** read-only; roster withdraw/replace via `--allowedTools` + `dispatch_dynamic_tool`; env-gated families. No read-only agent profile exists.
7. Relevant-looking backlog (unread in this pass): `2026-08-03-start-session-work-on-selected-branch-shares-the-owning-sessions-workt.md`, `2026-08-13-session-deletion-leaks-the-worktree-*.md`, `2026-08-13-tddy-web-a-failed-listsessions-is-indistinguishable-*.md`, `2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`, `2026-08-15-session-worktree-sync-deliberate-gaps.md`, `2026-08-17-session-agent-roster-the-sandbox-bridge-and-other-deliberate-gaps.md`, `2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md`, `2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md`, `2026-08-29-subagent-status-cannot-wait-for-a-turn-to-end.md`. `packages/tddy-tools/docs/code-issues/oversized-file-server.md` exists.

## Exploration 2: web session list, projects, activity pane, daemon lifecycle, backlog — 2026-10-22

**Agent**: parent Grep/Read (two Explore subagents for this scope were stopped by a safety classifier before returning; nothing from them is recorded)
**Scope**: session drawer grouping; SessionEntry/ProjectEntry/StartSession proto; projects storage; agent catalog; activity overlay; resume/idle; branch ownership; code-issues + todo scan

### Sequence

1. Grep `SessionList|SessionsScreen|SessionsPage|sessionList` in `packages/tddy-web/src/**/*.tsx` → `SessionsDrawerScreen.tsx`, `prstack/PrStackScreen.tsx`.
2. Grep `ProjectsScreen|ProjectsPage|ProjectList` → `components/projects/ProjectsAppPage.tsx`, `ProjectsScreen.tsx`.
3. Grep `activity ?pane|ActivityPane|activityPane|ActivitySidebar` → `gen/activity_pb.ts`, `useSessionActivity.ts`.
4. `ls components/sessions`; `wc -l` drawer/pane files.
5. Read `components/sessions/SessionDrawer.tsx:1-200`, grep structure 200-453.
6. Read `utils/sessionStackGroups.ts:1-120`.
7. Grep `message SessionEntry` in protos → `packages/tddy-service/proto/session.proto`; awk the message.
8. Grep `rpc|message|field` in `project.proto`.
9. Grep `rpc ` in `session.proto`, `worktree.proto`, `worktree_activity.proto`, `remote_git.proto`, `catalog.proto`, `pr_stack.proto`.
10. awk `StartSessionRequest`, `ResumeSessionRequest`, `ListAgentsResponse`, `AgentInfo`.
11. Grep `managed` in `docs/ft/daemon/remote-managed-worktree.md`; read `docs/ft/daemon/remote-codebase-mode.md:1-60`.
12. Grep `managedCodebase|managed_codebase` in web outside create-session files → none.
13. Grep `SessionActivitiesPane|useSessionActivity`; read `inspectorState.ts`, `SessionMainPane.tsx:150-260`, `SessionActivitiesPane.tsx:1-60`, `AgentActivityOverlay.tsx:1-60`.
14. Grep `idle_timeout|idle_stop|auto_stop|IdleTimeout` (Rust) and `agent_session_id|claude_session_id|resume_session_id|--resume`.
15. Read `tddy-agent-launch/src/svc_resume_session.rs` (grep), `svc_resume_claude_cli_session.rs:1-120` (grep), `tddy-cli-sessions/src/cli_session_manager/argv.rs:50-110`.
16. Read `tddy-task/src/idle.rs:1-40`; grep idle in `tddy-daemon-kernel/src/config/sections.rs`.
17. `ls tddy-projects/src`; read `project_storage.rs:15-60`; grep `set_project_*`; grep `fn list_agents`.
18. Read `tddy-worktree-service/src/branch_owner.rs:1-40`.
19. Grep `docs/dev/todo/` for list/drawer/project-group/activity/idle/auto-stop/read-only/default-agent; read 4 candidates; `ls */docs/code-issues` for affected packages; grep `Claimed by`.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `managedCodebase\|managed_codebase` | web src excl. create-session/gen | **none** — the web cannot tell a managed session today |
| Grep | `AgentActivityOverlay` | web | `SessionMainPane.tsx:8,279` (top-bar popover) |
| Grep | `idle_timeout…` | Rust | `tddy-task/src/idle.rs` (`IdleTimeoutTracker`), relay + index-daemon + LSP registry idle; **no session idle stop** |
| Grep | `--resume…` | Rust | `svc_resume_session.rs`, `svc_resume_claude_cli_session.rs`, `cli_session_manager/argv.rs`, `agent-backend/claude_argv.rs` |
| Grep | `fn list_agents` | Rust | `tddy-daemon-rpc/src/catalog/ports.rs:52`; test `list_agents_allowlist_acceptance.rs:137 list_agents_lists_harness_profiles` |
| Grep | `Claimed by` | code-issues of web, tools, tool-engine, daemon-rpc, agent-launch, session-lifecycle, worktree-service | **none claimed** |

### Inspected files

#### `packages/tddy-web/src/components/sessions/SessionDrawer.tsx`
Drawer list. Partitions sessions by activity, then stack-groups each side:
```tsx
const { active, remaining, activeCount, remainingCount } = partitionSessionsByActivity(sessions);
const isPartitioned = activeCount > 0 && remainingCount > 0;
...
<SessionDrawerSeparator testId="sessions-drawer-separator-active" count={activeCount}>
  <SessionPartitionBody partition={active} {...partitionBodyProps} />
</SessionDrawerSeparator>
```
`SessionStackGroup` renders a parent `SessionDrawerItem` + collapsible children (`useState(true)`), `data-testid="sessions-drawer-stack-${parentId}"`. Collapsed strip renders `CollapsedSessionDot` per session. Props carry `sessions: SessionEntry[]`, owning-host badge info, bulk-select.

#### `packages/tddy-web/src/utils/sessionStackGroups.ts`
`groupSessionsByStack(sessions) -> { groups: {parent, children}[], flat }` keyed by `orchestratorSessionId`; `partitionSessionsByActivity` splits on `connectionStatusForSession(s) !== "disconnected"` then groups each side.

#### `packages/tddy-service/proto/session.proto` — `SessionEntry`
Fields 1-33 incl. `project_id = 7`, `is_active = 6`, `repo_path = 4`, `agent = 12`, `session_type = 17`, `orchestrator_session_id = 21`, `branch = 28`, `codebase_daemon_instance_id = 29`, `agent_status = 31`, `last_activity = 32`, `ssh_config_host = 33`. **No managed flag, no role.** Service: `ListSessions`, `StartSession`, `StreamStartSession`, `ConnectSession`, `ResumeSession`, `SignalSession`, `DeleteSession`, `GetWorktreeSnapshot`.
`StartSessionRequest`: `project_id=3`, `agent=4`, `session_type=7`, `branch_worktree_intent=9`, `selected_branch_to_work_on=12`, `initial_prompt=13`, `permission_mode=14`, `managed_codebase=17`, `specialized_agents=18`, `repo_path=24`, `dangerously_skip_permissions=26`, `requested_session_id=33`, `sandboxed_codebase=39`, `provider_account_id=41`, `provider_built_in=42`. `ResumeSessionRequest{session_token, session_id}`.

#### `packages/tddy-service/proto/project.proto`
`ProjectService`: `ListProjects`, `CreateProject`, `AddProjectToHost`, `ListProjectBranches`, `SetProjectDefaultBranch`, `SetProjectAccounts`. `ProjectEntry{project_id=1, name=2, git_url=3, main_repo_path=4, daemon_instance_id=5, main_branch_ref=6, default_remote=7, (8 accounts), local_gh=9}`. Setter pattern: `Set*Request{session_token, project_id, <value>, daemon_instance_id}` → `{ProjectEntry project}`.

#### `packages/tddy-projects/src/project_storage.rs:22`
`ProjectData { project_id, name, git_url, main_repo_path, main_branch_ref: Option, remote_name: Option, host_repo_paths, accounts: Vec<AccountAssignment>, local_gh }` in `projects.yaml`; optional fields `#[serde(default, skip_serializing_if)]`. Setters `set_project_default_branch` (252), `set_project_accounts` (300); handler/service/lib trait fns in `handler.rs:35,42`, `service.rs:58,65`, `lib.rs:63,74`.

#### `packages/tddy-service/proto/catalog.proto`
`ListAgents(ListAgentsRequest{}) -> {repeated AgentInfo{id, label}}`; lists harness profiles (`list_agents_lists_harness_profiles`). This is the "agent type" the Projects screen would offer.

#### `packages/tddy-web/src/components/sessions/SessionMainPane.tsx`
Activities **pane** (`SessionActivitiesPane`) is the dormant-session base view; the **Agent Activity overlay** (`AgentActivityOverlay`, line 279) is a top-bar popover replaying `StreamAcpReplay` into a read-only `AgentChatView`. No right sidebar exists. File is 674 lines (code issue `oversized-file-session-main-pane.md`, unclaimed).

#### `packages/tddy-web/src/components/sessions/inspectorState.ts`
Inspector drawer `defaultInspectorOpen() → false`; reducer `open/close/toggle/expand/restore/select`.

#### `docs/ft/daemon/remote-codebase-mode.md`
Managed-codebase = agent cwd is a read-only temp dir with synced CLAUDE.md/skills; every file op goes through `ExecuteTool` against a worktree; `tddy-tools` re-exposes the exec catalog as `mcp__tddy-tools__*`; claude-cli enforced via `build_claude_allowlist` + `--disallowedTools`.

#### `packages/tddy-agent-launch/src/svc_resume_session.rs` / `svc_resume_claude_cli_session.rs`
`ResumeSession` branches: claude-cli (no LiveKit; managed resume goal via `LaunchSessions::managed_resume_goal`), cursor-cli (`--resume chat_id`), other CLI harnesses (opencode-cli, muse-cli), tool sessions (`resume_session_id: Some(req.session_id)`, agent def re-resolved, provider account re-resolved). **Resume of a stopped session already continues its agent conversation.**

#### `packages/tddy-task/src/idle.rs`
`IdleTimeoutTracker{ record_activity(), should_shutdown() }` — shared by relay daemon and LSP registry. No per-session agent idle stop exists.

#### `packages/tddy-worktree-service/src/branch_owner.rs`
"Which session owns a branch" rule: `SessionClaim{session_id, is_active, status, updated_at}` via `SessionListing` port; prefer active, then most-recent. Reusable for "is the target worktree's session active".

### Backlog / code-issue verdicts (draft)

- `docs/dev/todo/2026-10-14-project-account-assignment-reaches-one-host.md` — ⚠ During: a new per-project setter would reach one host the same way.
- `docs/dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md` — ⚠ During: a read-only home session cannot be enforced on cursor-cli; restrict home agents to enforceable harnesses or state the gap.
- `docs/dev/todo/2026-08-01-tddy-web-dead-session-surfaces.md` — — Unrelated (callerless `useSessionActivity`; we build on `useAcpReplay`).
- `docs/dev/todo/2026-08-23-the-action-tools-…` — Resolved already; unrelated.
- Code issues, all unclaimed, ⚠ During (put new code in new modules): `tddy-web/oversized-file-session-main-pane.md`, `tddy-tools/oversized-file-server.md`, `tddy-tool-engine/oversized-file-lib.md`, `tddy-daemon-rpc/oversized-file-project-coordinate-handlers.md`, `tddy-session-lifecycle/oversized-file-connection-service.md`, `tddy-agent-launch/complexity-svc-start-session-core-*.md`.

### Findings

1. Sessions already carry `project_id`; grouping per project is a pure web change on top of the existing activity partition + stack grouping. Project names come from `ListProjects`.
2. Nothing tells the web a session is managed → a `SessionEntry` field is needed for a managed-default sidebar.
3. Projects have a clean per-field setter pattern; a per-project home agent fits as `ProjectData.home_agent` + `SetProjectHomeAgent`; choices come from `ListAgents` (harness profiles).
4. Managed-codebase already gives "agent can only touch code via daemon tools"; a read-only home session = managed session whose exec catalog excludes write/shell tools.
5. Resume already continues the agent conversation; missing pieces are idle auto-stop (none exists) and click-to-resume for the pinned entry.
6. No tool lists sessions, reads another session's worktree, or runs commit/push/rebase/reset on a target worktree; `branch_owner.rs` gives the active-claimant rule for gating.

## Exploration 3: wiring points for the slices — 2026-10-22

**Agent**: parent Grep/Read
**Scope**: where the proto `SessionEntry` is built, whether `managed_codebase` is persisted, the exec allowlist, existing web specs

### Sequence

1. Grep `SessionEntry {` in Rust (non-test).
2. Grep `pub managed_codebase`; grep `managed_codebase` files.
3. Grep `fn build_claude_allowlist|pub fn tool_catalog`.
4. `ls packages/tddy-web/cypress/component` filtered drawer/project/activity/stack.
5. Read `tddy-session-activity/src/session_reader.rs:1-40` (fields); grep `managed` in `session_list_enrichment.rs`.
6. Read `tddy-agent-launch/src/svc_start_session_core.rs:455-500`.
7. Read field list of `tddy-changeset/src/session_metadata.rs:10-120`.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| Grep | `SessionEntry {` | Rust | proto entry built in `tddy-session-activity/src/session_list_enrichment.rs:478` (from `session_reader.rs:77` rows) and `tddy-agent-launch/src/session_coordinate_handlers.rs:400` |
| Grep | `managed_codebase` | Rust | start path only: `tddy-agent-launch/src/{placement.rs:32, svc_start_session_core.rs:470,488, cli_branch_starts.rs:59,164,210,244}`, `tddy-sandbox*`, `tddy-tools/src/mcp_primitives.rs` — **not** in `SessionMetadata`, **not** in `session_reader::SessionEntry` |
| Grep | `build_claude_allowlist` / `tool_catalog` | Rust | `tddy-sandbox-recipes/src/claude_cli.rs:239 build_claude_allowlist(subagent_enabled, replaced)`; `tddy-tool-engine/src/catalog.rs:16 tool_catalog()` |
| ls | cypress component | web | `SessionDrawer.cy.tsx`, `SessionsDrawerAcceptance.cy.tsx`, `SessionsDrawerActiveSeparator.cy.tsx`, `SessionsDrawerBulkDeleteAcceptance.cy.tsx`, `SessionsDrawerCrossHostAcceptance.cy.tsx`, `ProjectsScreenAcceptance.cy.tsx`, `AgentActivityAcpReplayAcceptance.cy.tsx` (+6 AgentActivity*) |

### Inspected files

#### `packages/tddy-changeset/src/session_metadata.rs` — `SessionMetadata` (`.session.yaml`)
Fields: `session_id, project_id, created_at, updated_at, status, repo_path, pid, tool, livekit_room, pending_elicitation, previous_session_id, session_type, model, cursor_chat_id, activity_status, hook_token, sandbox, agent, recipe, agents, agents_rev, legacy_specialized_agents, codebase_daemon_instance_id, codebase_session_id, agent_daemon_instance_id, agent_session_id, ssh_config_host`. No managed flag, no role.

#### `packages/tddy-session-activity/src/session_reader.rs`
Listing row `SessionEntry{session_id, created_at, status, repo_path, project_id, pid, is_active, tool, session_type, updated_at, livekit_room, previous_session_id, codebase_daemon_instance_id, codebase_session_id, ssh_config_host, …}` — read from `.session.yaml`, enriched into the proto in `session_list_enrichment.rs:478`.

#### `packages/tddy-agent-launch/src/svc_start_session_core.rs:460-500`
`classify_start_placement` passes `req.managed_codebase` into `PlacementRequest`; `managed_recipe_for` resolves a recipe only when managed. Placement decides; nothing records the decision on disk.

### Findings

1. A managed flag on `SessionEntry` needs `SessionMetadata.managed_codebase` written at start, carried through `session_reader` and `session_list_enrichment` — a three-crate vertical slice (changeset, session-activity, agent-launch) plus proto + web.
2. A `role` for the project-home session follows the same path.
3. The read-only home catalog is a filter over `tool_catalog()` plus a `build_claude_allowlist` variant; claude-cli is the enforceable harness.
4. Existing specs to extend: `SessionsDrawer*.cy.tsx`, `ProjectsScreenAcceptance.cy.tsx`, `AgentActivity*.cy.tsx`; daemon-rpc acceptance tests per RPC (`set_project_default_branch_acceptance.rs` model).
