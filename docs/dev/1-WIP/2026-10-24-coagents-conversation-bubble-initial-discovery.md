# Initial Discovery: Subagent conversation as one activity bubble (#coagents 4/8)

**Changeset**: [2026-10-24-coagents-conversation-bubble.md](./2026-10-24-coagents-conversation-bubble.md)
**Date**: 2026-10-24
**Passes**: 4 (whole-work passes, copied in full; node-specific passes follow as Exploration 5+)

## Combined Conclusions

- Every subagent conversation today — local (`open_local_agent_session`), owned-on-a-remote-clone
  (`open_owned_agent_session`) or in-jail (`SubagentRegistry::from_defs`) — is a
  `tddy_discovery::subagent::SpecializedSubagentSession`: an OpenAI-compatible chat-completion loop
  over `SpecializedAgentDef { model, base_url, api_key, tools, max_turns, replaces, … }`. The only
  other `SubagentSession` impl is `RemoteAgentSession` (`tddy-discovery/src/roster/conversation.rs:365`),
  a wire proxy to the facilitating daemon, which on the far side again builds a `SpecializedSubagentSession`.
- No def field can name a coding backend (`claude`, `cursor`, `codex-acp`, `opencode-acp`, `muse`,
  `claude-acp`). The roster doc separates "Agent" (`ListAgents` = coding backend) from roster entries
  by design (`docs/ft/daemon/session-agent-roster.md` § Terminology).
- Coding backends exist only as workflow `CodingBackend`s in `tddy-agent-backend`:
  `ClaudeAcpBackend` (`backend/acp.rs`), `CodexAcpBackend` (`backend/codex_acp.rs`),
  `OpenCodeAcpBackend` (`backend/opencode_acp.rs`) over ACP; `claude.rs` / `cursor.rs` headless `-p`.
  The three ACP workers are copies (`docs/dev/todo/2026-10-22-acp-backends-share-one-worker.md`).
- `spawn_conversation` / PR-stack `spawn-child` create full child sessions via
  `spawn_claude_cli_session_inner` — Claude CLI only, no agent choice, grill-me / pr-stack recipes only,
  no prompt/await channel back to the parent (`tddy-agent-launch/src/conversation_spawn.rs`,
  `svc_start_claude_cli_session.rs:154`).
- MCP subagent surface in `tddy-tools --mcp`: `subagent_new_session / _prompt / _await / _resume /
  _status / _diff / _pull / _end / _cancel` (ACP-shaped JSON, not the ACP crate). Grace period +
  `responseId` + `subagent_await` exist for long turns. Conversation worktrees exist for jailed
  conversations; daemon-run conversations still write the session worktree
  (`docs/dev/todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`).
- Remote agents: `name@daemon_instance_id`, owning daemon joins the session room, holds one clone per
  (session, remote daemon), reads local, mutations proxied back (`session-agent-roster.md` § Clones).
- Probable bug: `open_owned_agent_session` resolves via `resolvable_agent_defs()` (credential-free
  listing path) instead of `agent_def_for_spawn` — a keyed registry assistant owned by a remote daemon
  would 401 (`packages/tddy-session-agents/src/svc_start_hosted_agent_clone.rs:199`).
- The ACP *server* side exists too: `ProviderAcpAgent` (`tddy-acp/src/provider_agent.rs`) served by
  `ModelAcpService` (`tddy-model-registry/src/acp_service.rs`) for the Models & Agents chat — a
  reusable ACP conversation surface on the web.

## Exploration 1 — gap analysis (parent-driven grep/read)

### Sequence
1. `grep -rli ollama packages docs .agents` — locate local-model subagent support.
2. Read `docs/ft/coder/managed-codebase-subagents.md` (ACP→MCP mapping, architecture, known gaps, non-goals).
3. Read `docs/ft/coder/specialized-subagents.md` (def format, superseded notes).
4. Read `docs/ft/daemon/session-agent-roster.md` (summary, terminology, invoking, non-goals).
5. Read `docs/ft/coder/spawn-conversation.md`.
6. Read `packages/tddy-model-registry/docs/model-registry.md`, head of `packages/tddy-acp/src/provider_agent.rs`.
7. `grep -rn "SubagentSession for"` → `subagent.rs:1929` (`SpecializedSubagentSession`), `roster/conversation.rs:365` (`RemoteAgentSession`).
8. Read `packages/tddy-session-agents/src/svc_start_hosted_agent_clone.rs:140-240` (open_local / open_owned).
9. `grep "\"-p\""` → `tddy-agent-backend/src/backend/claude.rs:76`, `cursor.rs:193`.
10. Read `tddy-agent-launch/src/svc_start_claude_cli_session.rs:150-250`, `conversation_spawn.rs:40-120`.
11. `grep disallowed` in `tddy-sandbox-recipes/src/claude_cli.rs` — native `Task` is not withdrawn.
12. `ls docs/dev/todo` filtered for acp/subagent/backend/harness.
13. Read `tddy-session-agents/src/svc_resolve_listed_worktree.rs:51-192`.

### Findings
See Combined Conclusions. Relevant open TODOs:
- `docs/dev/todo/2026-10-22-acp-backends-share-one-worker.md`
- `docs/dev/todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`
- `docs/dev/todo/2026-09-27-a-subagent-turn-has-no-wall-clock-deadline.md`
- `docs/dev/todo/2026-09-30-an-abandoned-subagent-conversation-leaves-its-branch.md`
- `docs/dev/todo/2026-10-01-subagent-end-races-a-prompt-and-ignores-a-failed-cancel.md`
- `docs/dev/todo/2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md`
- `docs/dev/todo/2026-10-22-muse-code-headless-backend.md`
- `docs/dev/todo/2026-10-23-agent-activity-overlay-has-no-production-mount.md`
- `docs/dev/todo/2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web.md`

## Exploration 2 — coding backends, ACP workers, agent listing, jails (Explore agent)

### Sequence (abridged from the agent's 23 steps)
grep `impl CodingBackend for|trait CodingBackend`; read `tddy-agent-backend/src/backend/{mod,acp,acp_updates,codex_acp,opencode_acp,claude,claude_argv,codex,cursor,routed}.rs`,
`codex_acp/oauth.rs`, `provider_route.rs`, `cli_harness.rs`, `cli_harness_muse.rs`, `tddy-workflow/src/progress.rs`,
`tddy-coder/src/{run.rs,acp_agent.rs,acp_host.rs}`, `tddy-acp/src/{mapping,provider_agent}.rs`, `tddy-presenter/.../agent_session_runner.rs`,
`tddy-discovery/src/{backend,subagent,subagent_runtime,harness_profile,agent_list_mapping,catalog_service}.rs`, `tddy-tools/src/server.rs:2682-2817`,
`tddy-service/proto/catalog.proto`, `tddy-daemon-kernel/src/config/access.rs`, `tddy-daemon-rpc/src/catalog/{ports,agent_rows}.rs`,
`tddy-agent-launch/src/{harness_profile_request,cli_harness_jail,conversation_spawn,tool_session_spawn}.rs`, `tddy-sandbox-recipes/src/{plan,claude_cli,cursor_cli,opencode_cli,muse_cli,route_files}.rs`,
`tddy-daemon-sandbox/src/{workspace_tool_sandbox,sandbox_session}.rs`, `tddy-sandbox-runner/src/runner.rs:2395-2480`, `tddy-spawn/src/spawner.rs:1060-1110`,
`tddy-daemon-kernel/src/spawn_as_user.rs:180-240`, `svc_start_session_core.rs:295-480`; greps for `set_session_model|cancel(|mcp_servers|request.model`, `kill_on_drop`, `ProgressEvent::`, `--agent-kind`, `.invoke(`.

### Key excerpts
- `backend/mod.rs:873-905` — `trait CodingBackend { async fn invoke(InvokeRequest) -> Result<InvokeResponse>; fn name(); submit_channel(); action_invoke_cache_eligible(); list_models(); context_globs() }`.
- `mod.rs:429-499` `InvokeRequest { prompt, system_prompt[_path], goal_id, submit_key, hints, model, session: SessionMode{Fresh,Resume}, working_dir, agent_output[_sink], progress_sink, conversation_output_path, inherit_stdin, extra_allowed_tools, socket_path, session_dir, remote: RemoteToolEnv, … }` — `Default` exists.
- `mod.rs:859-869` `InvokeResponse { output, exit_code, session_id, questions, raw_stream, stderr }`.
- `mod.rs:38-48` `AnyBackend { Claude, ClaudeAcp, Cursor, Codex, CodexAcp, Stub, OpenCode }`; `mod.rs:168-193` one global `CHILD_PID` + SIGKILL `kill_child_process()`.
- `acp.rs` (`claude-acp`): `bunx claude-agent-acp` (307); route env only, stderr null (259-281); in-memory session map, never `load_session` (121,163-177); prompt only — `system_prompt` and `model` ignored (190); permissions auto `allow_once` (63-68).
- `codex_acp.rs`: system prompt merged (146); `load_session` resume (177-217); `codex login` retry on auth errors (185-243); bin `TDDY_CODEX_ACP_CLI`/`codex-acp`.
- `opencode_acp.rs`: `<bin> acp` (238); route files → `OPENCODE_CONFIG` (105-165); helpers `ensure_initialized`/`run_prompt`/`list_session_models` (287-337); `TODO(opencode-acp)` duplication (72-75).
- `acp_updates.rs`: `record_session_update` maps AgentMessageChunk/ToolCall/ToolCallUpdate/Plan → ProgressEvent; `allow_once()`.
- `claude.rs` (`claude`): `-p <prompt> --output-format stream-json --verbose` + `--permission-mode`, `--model`, `--session-id|--resume`, `--append-system-prompt[-file]`, `--allowedTools`, `--permission-prompt-tool mcp__tddy-tools__approval_prompt`, `--mcp-config` (temp config registering `tddy-tools --mcp`, 343-351); env `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, `TDDY_REMOTE_*` (405-424).
- `cursor.rs` (`cursor`): `[--plan] [--resume id] -p <prompt> [--model] --output-format stream-json --stream-partial-output --force --trust --approve-mcps` (177-206); writes `<wd>/.cursor/mcp.json` (209-227); cannot be routed.
- `codex.rs` (`codex`): `codex exec [-C wd] -s read-only|workspace-write --json [resume <thread>] …` (421-464); output parsed only after exit — no live progress (284-314).
- `provider_route.rs:22-43` `Harness` ids: claude, claude-acp, claude-cli, codex, codex-acp, opencode, opencode-cli, muse-cli, cursor, cursor-cli.
- `cli_harness.rs` `CLI_HARNESSES`: claude-cli, cursor-cli, opencode-cli, muse-cli (PTY TUIs). Muse has no headless backend (`docs/dev/todo/2026-10-22-muse-code-headless-backend.md`).
- `catalog.proto:29-32` `message AgentInfo { string id = 1; string label = 2; }` — no transport/capability field (`docs/dev/todo/2026-10-22-tool-session-provider-picker-needs-harness-metadata.md` asks for one).
- `harness_profile.rs` `HarnessProfile { name, label?, harness, provider_account, model }` in `<tddyhome>/agents/*.yaml` with `harness:`.
- `tddy-discovery/src/subagent.rs:62-141` `ContentBlock`, `StopReason { EndTurn, MaxTurnRequests, Cancelled, ContextExhausted, MaxTokens, YieldedToCaller }`, `PromptOutcome`; `187-221` `trait SubagentSession { take_turn, prompt, model, cumulative_usage, context_tokens, tail }`; `1991-2045` `SubagentRegistry::create` builds only from defs.
- `tddy-sandbox-recipes/src/plan.rs:23-38` `SandboxRecipe { Generic, ClaudeCli, CursorCli, Shell, RunnerPty, OpenCodeCli, MuseCli }` — no codex/ACP recipe; in-jail agents always run in a PTY (`runner.rs:2401-2480`). Headless tool sessions run unjailed as the OS user (`spawn_as_user.rs:194-225`).

### Findings
| id | transport | resume | live progress | cancel | model honoured | tddy-tools MCP injected |
|---|---|---|---|---|---|---|
| `claude` | `-p` stream-json | `--resume` | yes | global SIGKILL | yes | yes (`--mcp-config`) |
| `claude-acp` | ACP (claude-agent-acp, SDK-based) | in-memory map only | yes | **none** | **no** | **no** |
| `cursor` | `-p` stream-json | `--resume` | partial | global SIGKILL | yes | yes (`.cursor/mcp.json`) |
| `codex` | `exec --json` | `exec resume` | **none until exit** | global SIGKILL | yes | no |
| `codex-acp` | ACP | `load_session` | yes | **none** | **no** | **no** |
| `opencode` | ACP | `load_session` | yes | **none** | **no** | **no** |
| `muse` | PTY only | — | — | — | — | — |

- `CodingBackend::invoke` is already "one prompt turn → final text + session id"; missing: cancel, stop reason, usage, MCP server injection (ACP `mcp_servers`), a capability descriptor, permission routing (all backends auto-approve).
- The three ACP workers duplicate spawn/connect/initialize/new-or-load/prompt; only `acp_updates.rs` is shared.
- No metadata says whether a listed agent is ACP, `-p`, PTY or a def.
- No sandbox recipe for headless/ACP agents; jailed agents are PTY-only today.

## Exploration 3 — web activity pane, ACP renderers, pickers, transcripts (Explore agent)

### Sequence (abridged from 45 steps)
Read `docs/ft/web/agent-activity-pane.md`, `docs/ft/coder/acp-replay-lazy-tool-bodies.md`, `docs/ft/web/session-agent-catalog-fan-out.md`,
`packages/tddy-web/docs/{session-agent-conversation,session-agent-tree,inactive-session-activities}.md`, the todos
`2026-07-26-…tool-detail-dialog`, `2026-10-23-…overlay-has-no-production-mount`, `2026-08-30-agent-conversation-tabs-four-follow-ups`,
`2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web`; web `SessionWithActivitySidebar.tsx`, `SessionActivitySidebar.tsx`,
`activitySidebarState.ts`, `SessionMainPane.tsx`, `chat/{useAcpReplay,acpReplayProjection,AgentChat,AgentTranscriptView,useAgentChat,useAcpSession,useAcpToolCallDetail,chatEntryPresentation,toolCallPresentation}.ts(x)`,
`AgentActivityDetailDialog.tsx`, `PrStackChat.tsx`, `ModelChatDialog.tsx`, `useAgentConversation.ts`, `agentConversationTranscript.ts`, `SessionAgentConversationPane.tsx`,
`agentConversationTabs.ts`, `useSessionAgentConversations.ts`, `SessionRuntime.tsx`, `SessionAgentTree.tsx`, `useSessionAgentRoster.ts`, `useAvailableAgents.ts`,
`useSelectableAgents.ts`, `CreateSessionAgentPickerSection.tsx`, `CreateSessionPane.tsx`, `createSessionRequest.ts`; protos `activity.proto`, `session_agents.proto`,
`catalog.proto`, `session.proto`, `exec_tools.proto`, `types.proto`; Rust `tddy-service/src/acp_replay.rs`, `convert_acp.rs`, `tddy-agent-backend/src/stream/claude.rs`,
`tddy-session-activity/src/service/{hook_reports,rpc}.rs`, `svc_activity_ports.rs`, `tddy-tools/src/{server,mcp_primitives}.rs`, `tddy-discovery/src/subagent/transcript.rs`,
`subagent_runtime.rs`, `tddy-daemon-sandbox/src/sandbox_session.rs`, `tddy-changeset/src/agent_activity.rs`, `tddy-daemon-rpc/src/{exec_tool,catalog}/*.rs`.

### Key excerpts
- `SessionMainPane.tsx:266` mounts `SessionWithActivitySidebar` → `useAcpReplay({sessionId,…})` (`:73`) → `SessionActivitySidebar` → `AgentChatView readOnly` → `AgentTranscriptView`; single `detail` state opens `AgentActivityDetailDialog` (`:87,145-162`). Open by default when `managedCodebase` (`activitySidebarState.ts:10`).
- `useAcpReplay.ts:102-177` count feed `COUNT_THEN_LIVE` + transcript feed `TAIL_THEN_LIVE` (100), `getAcpReplayPage` paging (184-220); `daemonInstanceId: ""` hard-coded (109,146,199).
- `acpReplayProjection.ts:59-94` one `ChatMessage{from:"tool", text: call.title, toolStatus, toolCallId}` per `tool_call_id`; `tool_call_update`/`plan` ignored in replay (144-148).
- `useAgentChat.ts` `ChatMessage.from: "user"|"agent"|"goal"|"activity"|"tool"` (closed union); `AgentTranscriptView.tsx:99-103` only `from === "tool"` is clickable.
- `acp_replay.rs:99-112` `read_session_transcript` merges `acp-transcript.jsonl` + `agent-activity.jsonl` (`frame_for_agent_activity`, 357-398); title = `tool_use_detail(name)` or bare name — `mcp__tddy-tools__subagent_prompt` renders as a bare one-line bubble, one per call, ungrouped.
- `sandbox_session.rs:251-330` `DaemonToolHandler::execute(session_id, conversation_id, …)` appends subagent tool calls to the **parent** `agent-activity.jsonl` with no `conversation_id` (`AgentActivityRecord`, `tddy-changeset/src/agent_activity.rs:57-95`, has none).
- `activity.proto:299-306` `StreamAcpReplayRequest { session_token, session_id, daemon_instance_id, mode, page_size }` — no agent/conversation axis; `svc_activity_ports.rs:263-273` refuses peer-addressed `StreamAcpReplay` (page/detail are peer-forwarded).
- `session_agents.proto:23-78` `SessionAgentService` (Attach/Detach/List/Stream, Open/Prompt/Resume/Cancel AgentConversation, ReportAgentCloneState, ReportAgentConversationState); `90-145` `AgentConversationChunk`, `AgentMessageDescriptor`; `309-347` `SessionAgentEntry` has no conversation ids.
- `catalog.proto:29-32` `AgentInfo{id,label}`; `73-90` `SubagentInfo{name,label,model,daemon_instance_id,agent_id,replaces,tools}`; `session.proto:492-495` `specialized_agents = 18`.
- `tddy-daemon-rpc/src/catalog/agent_rows.rs:21-37` `ListAgents` = allowlist + registry assistants + harness profiles; `ports.rs:167-185` `ListSubagents` = `resolvable_agent_defs` only; `agent_def.rs:212-231` a `harness:` YAML is skipped by the def loader.
- Pickers: `CreateSessionAgentSelect` (single, `ListAgents`) and `CreateSessionAgentPickerSection` (multi, `ListSubagents`, gated on `managedCodebase`, claude-cli/cursor-cli only, `createSessionRequest.ts:163,229`); `AgentPicker.tsx:51` add-agent uses `useAvailableAgents`.
- `tddy-tools/src/server.rs:1763-1855` `subagent_new_session_tool`: no local def → `open_remote_agent_session` (daemon `OpenAgentConversation`); else in-process `SubagentRegistry` + `report_local_conversation_state`.
- `tddy-discovery/src/subagent/transcript.rs` — subagent history is memory only; no RPC returns a subagent transcript.
- Existing renderers: `useAcpReplay`+`AgentTranscriptView` (only read-only one, session-keyed); `useAcpSession` (interactive LiveKit `AcpService.Session`, PrStackChat); `ModelChatDialog` (interactive daemon ACP, own markup); `SessionAgentConversationPane` (plain text). Only `AgentChat.stories.tsx` exists in Storybook.

### Findings
- The web read path is built around a **session dir**: `StreamAcpReplay(session_id)` → `read_session_transcript(session_dir)`. A conversation that persisted ACP frames to its own transcript file and was addressable by `(session_id, conversation_id)` would render with the existing `AgentTranscriptView` + detail dialog unchanged.
- Today nothing groups `subagent_*` calls by conversation; the bubble text is the bare tool name and the body (with `sessionId`) is stripped from stream frames.
- A "single subagent bubble" needs: a new `ChatMessage.from` kind (or a tool kind) projected from frames carrying a conversation id; a click path besides `onToolClick`; a dialog/pane hosting `useAcpReplay` keyed by conversation.
- Permission answers from the web need an RPC on the conversation (none exists).

## Exploration 4 — roster, tddy-tools subagent runtime, daemon conversations, clones, transports (Explore agent, sonnet)

### Sequence (abridged from 20 steps)
Read `tddy-tools/src/{server,mcp_primitives,subagent_end,subagent_pull,worktree_sync_port,lib}.rs`; `tddy-discovery/src/{subagent,subagent_runtime,agent_def,catalog_entry,catalog_service,agent_list_mapping,harness_profile,backend,roster}.rs`,
`roster/{registry,seed,roster_seed,conversation,link,stream}.rs`, `subagent/{yield_condition,turn_request,turn_start,worktree_reset,worktree_sync}.rs`; `tddy-subagent-worktree/src/{lib,run,tool_effect,worktree}.rs`;
`tddy-session-tool-client/src/{lib,conversation}.rs`; `tddy-session-lifecycle/.../{local_exec_tools,svc_session_agent_port_adapters,svc_peer_routed_session_agents}.rs`; `tddy-daemon-sandbox/src/conversation_tool.rs`;
`tddy-session-agents/src/{ports,agent_conversations,service,svc_start_hosted_agent_clone,session_agent_clone,agent_roster,agent_records,svc_resolve_listed_worktree,svc_turn_end_reporter,spawn_agent_def,peer_session_answer,exec_tool_caller,hosted_clone_start,session_agent_roster}.rs`;
`tddy-changeset/src/session_agent.rs`, `session_metadata.rs`; protos `session_agents.proto`, `catalog.proto`; `tddy-daemon-rpc/src/catalog/ports.rs`; `tddy-agent-launch/src/{jail_env_builders,conversation_worktree_op}.rs`;
`tddy-sandbox-app/src/spawn.rs`; `tddy-service/src/session_agents.rs`; `tddy-tool-engine/src/exec_tool_entry.rs`; `tddy-daemon-kernel/src/privilege_drop.rs`; docs roster / remote-managed-worktree / remote-codebase-mode / harness-profiles / managed-codebase-subagents; subagent TODOs 2026-09-26..10-03.

### Key excerpts
- `server.rs:1763-1853` `subagent_new_session`: `roster.open_conversation_as` (1785) → `roster.local_def_for(&entry)` else `open_remote_agent_session` (1794-1799, daemon `OpenAgentConversation`) → else in-process `SubagentRegistry::from_defs` (1826-1831). Router 2682-2855; tools advertised only with a non-empty roster; `agent` schema enum rebuilt per `tools/list` (2451-2482).
- `server.rs:2013-2092` `take_a_turn` → `DeferredTurn` + `tokio::spawn(run_turn)` + `wait_for_turn(grace)`; `SUBAGENT_PROMPT_GRACE = 25s` (1728).
- `subagent.rs:186-221` `SubagentSession { take_turn(TurnRequest), prompt, model, cumulative_usage, context_tokens, tail }`; `80-107` `StopReason`; `111-141` `PromptOutcome { stop_reason, content, usage, messages, clamped_max_turns, fired_condition, yielded_message_id, worktree_reset, worktree_sync }`; `781-812` `SubagentConfig`; `1991-2046` `SubagentRegistry` (only `SpecializedSubagentSession`).
- `subagent.rs:1598-1622` yield fires after a tool result is appended (`YieldedToCaller`, `fired_condition`, `yielded_message_id`); `turn_request.rs:131-142` resume replacement.
- `subagent_runtime.rs:52` `SubagentConversation{ session: Arc<Mutex<Box<dyn SubagentSession>>>, remote }`; `157` process-wide table; `211` `PendingTurns`; `426-460` `prompt_outcome_json`; `625` `run_turn`.
- `tddy-subagent-worktree`: lazy worktree at `<session worktree>/tmp/subagent-worktrees/<conv>` branch `tddy/subagent/<session>/<conv>` (`worktree.rs:20`); per-call rule `run.rs:50-92`; read-only set `tool_effect.rs:20-25`; daemon applies it via `run_in_conversation` (`local_exec_tools.rs:195-231`) only when `ExecuteToolRequest.conversation_id` is set — in-jail loops only.
- `svc_start_hosted_agent_clone.rs:269-334` `local_agent_codebase_access` → `conversation_id: ""`, `run_exec_tool_locally` on the session worktree; `242-265` `owned_agent_codebase_access` → reads clone, writes `execute_tool_on_facilitator` (`session_agent_clone.rs:287-361`, `StreamExecuteTool`).
- `service.rs:783-884` `OpenAgentConversation`: `open_owned` → `open_local` (record on this daemon) → `peers.open` (remote); `agent_conversations.rs:19-38` `AgentConversation::{Local{session,closed}, Remote{daemon_instance_id}}`, host-global map; `service.rs:162-285` prompt/resume `take_a_turn`; `925-970` cancel.
- `service.rs:551-606` `AttachSessionAgent`: `catalog.record_for` → `admission.admit` (clone claimed when `daemon_instance_id != local`, `svc_session_agent_port_adapters.rs:59-91`) → `rosters.attach`.
- `tddy-changeset/src/session_agent.rs` `AgentId{name,daemon_instance_id}` (`name@daemon`), `SessionAgentRecord{agent_id,name,daemon_instance_id,label?,model,replaces,tools,codebase_session_id?}`; `session_agents.proto:309-347` `SessionAgentEntry` fields 1-12; `catalog.proto:73-91` `SubagentInfo` 1-7.
- Wire-spelled enums: stop reasons `service.rs:424-433` + `conversation.rs:508-526`; roles `service.rs:536` + `conversation.rs:490` — unknown spellings refused.
- `harness_profile.rs` `HarnessProfile{name,label,harness,provider_account,model}` — `docs/ft/daemon/harness-profiles.md`: "not offered by ListSubagents and cannot be attached to a session's roster"; `agent_def.rs:221-231` loader skips them; `SpecializedAgentDef` `deny_unknown_fields`, `base_url` required.
- Transports (`tddy-session-tool-client/src/lib.rs:236-301`): `SandboxIpc` (`TDDY_SANDBOX_TOOL_IPC`) → LiveKit (`TDDY_REMOTE_LIVEKIT_*`) → `DaemonUds` → `DaemonHttp`; `link.rs:85-88` refuses roster/conversation RPCs over `DaemonHttp`; jail allowlist `IN_JAIL_RELAYABLE` (`tddy-service/src/session_agents.rs:82-89`) + `IN_JAIL_RELAYABLE_EXEC_TOOLS` (`exec_tool_entry.rs:17-20`).
- OS user: daemon-run loop runs inside `tddy-daemon`; exec tools run as the daemon identity (no privilege drop in `tddy-tool-engine`); session/agent processes use `spawn_as_user` / supervisor (`tool_session_spawn.rs:241-262`, `privilege_drop.rs`).

### Findings
- A coding-agent roster entry has no seed def, so `subagent_new_session` already routes it to the daemon (`OpenAgentConversation`) — the MCP surface needs no new tool to reach it.
- The plug-in seam is a new `impl SubagentSession` built in `open_local_agent_session` / `open_owned_agent_session`; `AgentConversation::Local` already holds `Box<dyn SubagentSession>`.
- OpenAI-loop-only request features (`fromMessageId`, `correction`, `replacement`, `yieldConditions`) must be refused with a typed error by a coding-agent session.
- Daemon-run conversations write the session worktree (`conversation_id: ""`) — that IS the "shared worktree" default; the separate-clone option means wiring `ConversationWorktrees::ensure/run_in_conversation/pull/remove` for daemon-run conversations (`conversation_worktree_op.rs` already implements the ops).
- Local clones are only claimed for remote owners today; a local "clone" placement is a new admission/placement branch.
- A coding-agent process must be spawned as the session's OS user (spawn worker / supervisor path), not inside the daemon.
- Remote-codebase mode: main agent local, codebase on a remote daemon `workspace` session; tddy-tools there uses LiveKit/`DaemonHttp` via the relay, and `DaemonHttp` refuses conversation RPCs — the remote node must route conversations to the codebase daemon.
- Wire additions: `SessionAgentEntry` 13+, `SubagentInfo` 8+, `SessionAgentRecord` serde-defaulted `kind`, stop-reason table growth for a permission yield, `IN_JAIL_RELAYABLE` for any new conversation RPC.
