# Initial discovery — orchestrators pause, delete and clean up sessions (`session-lifecycle-tools`)

**Node**: `#orchestrators` 5/11 · changeset [`2026-11-10-session-lifecycle-tools.md`](./2026-11-10-session-lifecycle-tools.md)

## Combined conclusions (this node)

- No pause/mark field in `SessionMetadata`; `SessionEntry` last field is 36 → mark at 37.
- `DeleteSession` RPC (`session.proto:48`, `session_deletion.rs`), `RemoveWorktree` / `CleanWorktree` /
  `RestoreSessionWorktree` RPCs (`worktree.proto:13,24,25`) exist; no exec tools.
- The worktree todo names the functions to reuse (`clean_worktree_under_repo`,
  `remove_worktree_under_repo`, the restore path) and the test file
  (`tddy-daemon-rpc/tests/worktree_tools_acceptance.rs`).
- Resume clears `paused` in the shared daemon resume path, so this node needs none of n4's behaviour.

## Exploration 1 — whole-work discovery for `#orchestrators` (copied 2026-11-10)

The full dump the stack was planned from: interview record, combined conclusions and four Explore
passes (project-home daemon, PR stacks, web drawer, deferred-work cross-check).


**Temporary whole-work dump.** Not a changeset companion; never staged on a stack branch. Copied into
each node's `{slug}-initial-discovery.md` as Exploration 1, then deleted at the pass-1 checkpoint.

### Combined conclusions

1. **One main per project is hard-coded in three places**: scalar `ProjectData.home_session_id`, one
   home worktree path, a daemon-global `ENSURE_LOCK`. Main stays as is; orchestrators need no record —
   they are ordinary sessions carrying a role.
2. **Role privileges key on the exact string `project-home` at six sites.** A new role value
   `orchestrator` plus one predicate (`SessionToolRole::Orchestrator` covering both) gives orchestrators
   the guard, catalogue, wiring, token mint and cross-host owner check, while auto-stop keeps matching
   `project-home` only — which is exactly "auto-stop main only".
3. **`session_role` is refused at `refuse_a_requested_role`.** "Flag is the role": allow
   `session_role: orchestrator` from people and from orchestrator-role agents; keep `project-home`
   daemon-only. The daemon still forces main's placement (`on_home_worktree`).
4. **Tool code is caller-relative** (ledger, lease screen, self-filter), so N orchestrators on one
   worktree mostly coexist; but another orchestrator is a valid *write* target on the shared home
   worktree today — must be refused.
5. **No effort parameter exists anywhere**; needs proto field 48, a spec slot, the web picker, the exec arg.
6. **`/clear` is undetected**: the SessionStart hook is installed but `source` is dropped and the agent's
   conversation id is never forwarded; claude resumes by the tddy id. Fix: agent-YAML-declared rotation,
   `.session.yaml` keeps the full list of conversation ids (user, 2026-11-10), resume uses the newest.
   `SessionMetadata` is `deny_unknown_fields`.
7. **Missing session-control tools vs `/yolo-pr-stack`**: send a turn to a running session, resume a
   stopped one, pause/status marker, delete session, remove worktree. Terminate, follow, answer, wait
   exist.
8. **Stacks: ~60 production sites in 8 crates** read/write `changeset.stack`, nearly all through
   `update_stack_atomic` / `read_changeset().stack` → a store seam keyed by `(project_id, stack_id)` at
   `<data>/projects/<project_id>/stacks/<stack_id>.yaml` (dir exists for notifications). No lock today.
   Journal, per-node docs, `stack-plan.yaml` stay keyed to the orchestrator session dir.
9. **`pr_*` tools derive everything from `TDDY_SESSION_DIR`/`TDDY_REPO_DIR`** in tddy-tools (server.rs
   2,677 lines). Daemon exec-tool family pattern exists (tddy-tool-engine module + port + catalogue +
   `run_locally` + `served_on_target_host`). Git runs in the wrong place: `pr_set_parents` rebases in
   `TDDY_REPO_DIR`, `pr_resolve_conflicts` trusts an agent path; merge/close/repoint are API-only.
   Daemon push lacks remote/token (blocking todo).
10. **Web** reads stacks only via `stack_plan_json` (SessionEntry 23 + PrStackService responses); all
   PrStackService RPCs are addressed by orchestrator `session_id`.
11. **Web drawer**: group header is one `<button>`; composer controls exist in tddy-session-create-web
   (mostly unexported); no centred composer; projects without main or sessions get no group.
12. **Overlap**: draft #876 (`feature/agent-tools/sweep`) edits `connection_service.rs` and
   tddy-agent-launch start code.

### Interview record (2026-11-10)

| Question | Answer |
|---|---|
| Main special or first of N | **Main stays** (pinned, `home_session_id`, `EnsureProjectHomeSession`). Main must support `/clear` and record the new conversation id, and must be able to create orchestrator sessions |
| Role naming / grant | Orchestrators are **regular sessions**, creatable from New session, with a **"Session orchestrator" flag**; **the flag is the role** (requestable on `StartSession`) |
| Role guard (challenge) | **Open to people; agents need the role** — an agent-credential caller may set the flag only when it is itself an orchestrator. Daemon still forces the placement (claude-cli, home worktree, read-only) |
| Auto-stop | **Main only** |
| `/clear` scope | **All sessions**, generic, declared in **agent YAML**, not Claude-specific |
| `+` on project entry | Opens an **agentic composer screen**: prompt box vertically centred (screenshot: "What should we build next?"), limited controls: **model + effort, provider account, host picker, attachments**; send starts the orchestrator with the text as initial prompt |
| Gear | **Project settings pane** (`#/sessions/project/<id>/settings`): main setup / Replace main moves there, plus defaults the `+` composer pre-fills |
| Stack store | **Per host, forwarded**: `(stack_id, daemon_instance_id)`, owning host serves, peer-forwarded like git tools |
| Migration | **Daemon startup sweep** of every `changeset.stack` in every mapped user's sessions dir; no dual-read |
| Tool access | **Daemon runs git; split by need**: orchestrators + pr-stack orchestrators get every `pr_*`; rebase/merge/repoint daemon-side in the node's worktree; `pr_resolve_conflicts` from read-only reports + delegates to the node's session; ordinary sessions none; stack-node children read-only `pr_stack_status` + `pr_read` |
| Full session control (mid-interview addition) | Home + orchestrator sessions must **fully control sessions as subagents** — everything `/yolo-pr-stack` does to its subagents: start (model/effort, existing branch worktree), send follow-up message, completion notification, read result, resume, stop, **mark paused** etc. "All the tools should be there" |

### Exploration 4 — deferred-work cross-check (Step 2b)

Explore pass over `docs/dev/todo/` (440 files, keyword-matched, most judged on their first 14–30
lines), `packages/*/docs/code-issues/`, `docs/dev/1-WIP/`, and open PRs.

**Blocking**
- `2026-10-25-start-session-orchestrator-recorded-after-start.md` — `record_orchestrator`
  (`svc_start_session_core/orchestrator_record.rs`, `TODO(start-parity)`) rewrites
  `orchestrator_session_id` after the agent runs; race. Orchestrator links rest on it.
- `2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md` — `confirmed_home_caller`
  (`exec_tool/project_home_remote.rs`) accepts only a peer-listed `session_role == project-home`.
- `2026-10-29-spawner-read-scope-is-same-host-only.md` — spawner scope check is same-host only;
  blocks cross-host full session control.
- `2026-10-23-project-home-git-push-remote-and-token.md` — daemon-run git pushes to `origin` with the
  checkout's credentials, no token channel; blocks daemon-side merge/repoint/rebase for `pr_*`.

**Do-during** — auto-stop untracked processes; running heuristic (`stop_previous_main` TODO);
sandboxed main role wiring; StartSession MCP schema free strings (`project_home_tool_defs()` before
ListExecTools); orchestrated `tool` start refused; forwarded start cancel "left in place";
conversation id not bound to session (`PromptAgentConversation`/`ResumeAgentConversation` by id
alone); project card setup link host; New-session form 8 of 10 recipes; claude-cli transcript tool
calls only; `stack-progress.json` documented but unwritten; `QueryBranch` local-only legs; PR-Stack
full-control follow-ups (split `pr_stack/mod.rs`, lift `server/subagent.rs`); `tddy-tools server.rs`
3.9k lines; generalize `pr_stack_spawn_args`; cursor-cli orchestrator link never reaches SessionEntry;
agent catalog fan-out (`AgentInfo` has no `kind`/`tools`); claude agent tab shares session hooks;
peer-forwarding tests use LiveKit / cheap-RPC policy.

**Already answered** — `2026-10-23-session-drawer-grew-past-500-by-project-groups.md` (drawer moved to
tddy-session-web, `SessionDrawer.tsx` 333 lines; pointed-at code issue NOT FOUND) → deletable.

**Code issues** — none relevant is claimed. Over budget in the stack's path:
`tddy-tools/src/server.rs` 2,677 prod lines; `tddy-session-lifecycle` `connection_service.rs` 525;
`tddy-daemon-rpc` `pr_stack/ports.rs` 808; `tddy-session-create-web` `CreateSessionPane.tsx` 581;
`tddy-agent-launch` `start_session_on_request`/`start_here` rank-1 CRAP; exec-tool ports
(`execute_tool`, `list_exec_tools`, `stream_execute_tool`) complexity; `tddy-pr-stack`
`pull_base_into_node_branch`, `repoint_planned_pr_node`; `tddy-prstack-web` `PrStackScreen.tsx`;
`tddy-tools` `session_hook.rs` 535; `tddy-workflow-recipes` `squatting-pr-stack-data-model.md`
(unclaimed; PR-stack data model sits in workflow-recipes).

**1-WIP** — only stale changesets touch `CreateSessionPane`/`StartSession`
(`2026-09-20-sandboxed-codebase-managed-workflow.md`).

**Open PRs** — **#876** (draft, `feature/agent-tools/sweep`, base master) overlaps
`connection_service.rs`, `coding_agent_launcher.rs`, `svc_ensure_project_available_for_start.rs`;
land or base on it first. #176, #121 stale.

### Exploration 3 — web: drawer, project home, create-session form

Queries: `grep -rln` for each named symbol under `packages/` (ts/tsx), `effort|thinking|reasoning`,
`sessionRole|PROJECT_HOME_ROLE`, `composer|textarea`, `mountWithRpc|anInMemoryRpcBackend`,
`find *.stories.tsx`; package.json descriptions.

**Drawer (tddy-session-web)**
- `SessionProjectGroup.tsx:20-46` — header is ONE `<button>` (`sessions-drawer-project-group-header-${key}`,
  `aria-expanded`, `onClick={onToggle}`); "+"/gear cannot nest → header becomes a row (toggle + sibling
  action buttons). `projectGroupKey` line 6; `ProjectSessionGroup` in `utils/sessionProjectGroups.ts`;
  collapse `projectGroupCollapse.ts`.
- `SessionDrawer.tsx:238-263` renders `projectHomeSlot(group, projectHomes)` → `SessionProjectGroup` →
  `ProjectHomeEntry` + `SessionGroupBody`. Props `onCreateSession?` (24), `projectHomes?` (72),
  `onEnsureProjectHome?` (74), `onSetUpProjectHome?` (76), `onReplaceProjectHome?` (78); global
  `sessions-drawer-new-btn` (214-223); `withProjectHomeGroups` at 116.
- `ProjectHomeEntry.tsx:19-84` (`sessions-drawer-project-home-${projectId}`, error testid);
  `ReplaceMainButton` 87-106 (lucide `Replace`, `sessions-drawer-project-home-replace-${id}`), shown when
  `hasMain && onReplaceProjectHome` (69).
- `useProjectHomeSetup.ts:17-36`, `useEnsureProjectHome.ts:30-80` (`ensureProjectHomeSession`, `isNoMain`
  83), `useSessionsDrawerProjectHomes.ts:20-32`.
- `utils/projectHomeGroups.ts`: `ProjectHomeInfo` (6), `projectHomesOnHost` (23), `PROJECT_HOME_ROLE` (38),
  `projectHomeSlot` (53, decides by `homeSessionId`), `withProjectHomeGroups` (78) — a project with no
  main and no session gets **no group** → no "+"/gear unless the rule changes.
- `SessionsDrawerScreen.tsx`: 113-115 hooks; 291 `handleCreateSession` → `SESSIONS_NEW_ROUTE`; 314-331
  project-home handlers; 380-385 drawer props; 390-398 detail switch (`setup ? ProjectHomeSetupPane :
  unknownSession ? … : SessionMainPane`); mode from `useSessionsDrawerInspector.ts:23`.
- `SessionDrawerItem.tsx`: no role badge; label 62, host badge 69-75, codebase-host badge 79-86, meta
  line 103-110; badge helpers `SessionDrawerGroups.tsx:33-48`.

**Create-session (tddy-session-create-web)** — `ProjectHomeSetupPane.tsx:26` (props 54-66, own clients
81-86, renders `CreateSessionPane … projectHome={…}` 106-120); `useProjectHomeStartSubmit.ts:22-47`;
`CreateSessionPane.tsx:57-581` (props `createSessionPaneProps.ts:91-131`, `CreateSessionInitialValues`
37-89, `CreateSessionProjectHome` 134-143). Main variant: type picked by agent capability
`project_home_main` (174-193), `whyMainCannotRun` (`projectHomeSessionTypes.ts:19`), host/project
hidden (412), stack/branch/attachments hidden (501-540), `placementFixed` (496), submit "Start main
session" (561); spec `buildProjectHomeStartSpec` (`createSessionRequest.ts:260`), regular
`buildStartSessionRequest` (97).
Reusable controls: `CreateSessionModelField.tsx:15` + `useCreateSessionModel.ts:40` + `useAgentModels.ts`;
`CreateSessionProviderField.tsx` + `useProviderRoutes.ts:31,79,124`; host select inside
`CreateSessionHostAndProjectFields.tsx:24` (36-56, needs splitting); attachments
`CreateSessionAttachmentsSection.tsx`, `attachments/{AttachmentDropZone,SessionAttachmentList,HostDocumentPicker}.tsx`,
`hooks/useSessionAttachments.ts:220`; initial prompt textarea `CreateSessionCliFields.tsx:199-211`.
Most are **not exported** from `index.ts` → composer belongs in tddy-session-create-web.
**Effort/thinking: NOT FOUND** anywhere in web or protos (only "best-effort" in exec_tools.proto).
**Centred composer: NOT FOUND**; closest `tddy-chat-web/src/LiveAgentChatView.tsx:214-231` (bottom bar).

**Routes (tddy-client-web/src/appRoutes.ts)** — `SESSIONS_DRAWER_ROUTE` (33),
`SESSIONS_PROJECT_HOME_SETUP_ROUTE` (41), `PROJECT_HOME_REPLACE_SEGMENT` (44),
`sessionsProjectHomeSetupPath` (50-56), `SESSIONS_NEW_ROUTE` (67), `parseSessionsProjectHomeSetup`
(83-93), `isSessionsDrawerPath` (104-108; new routes must be accepted). Re-exported by
`tddy-web/src/routing/appRoutes.ts`; consumer `tddy-workspace-web/src/projects/ProjectMainSessionLink.tsx:18`.
Doc drift: session-drawer.md:113-118 names wrong packages for setup pane/routes.

**Proto** — `SessionEntry.session_role` field 35; `StartSessionRequest.session_role` field 43 (refused
unless daemon-set); `EnsureProjectHomeSessionRequest.start`. TS gen `tddy-client-web/src/gen/session_pb.ts`.

**Tests** — Cypress `tddy-session-web/cypress/component/`: `SessionsDrawerProjectHomeAcceptance.cy.tsx`
(`anInMemoryRpcBackend().onUnary(SessionService.method.ensureProjectHomeSession…)`, `mountWithRpc`),
`ProjectHomeSetupPaneAcceptance.cy.tsx` (full screen, `aSessionServiceBackend`, `withSelectedDaemon`),
`ProjectHomeReplaceAcceptance`, `SessionsDrawerProjectGroupsAcceptance`; page objects
`cypress/support/pages/{projectHomeSetupPage,sessionsDrawerPage,createSessionPage}.ts`; test ids
`cypress/support/testIds.ts` 754-790. Create-web specs `tddy-session-create-web/cypress/component/`
(model, provider, host, attachments, request builder parity). Stories in `packages/tddy-storybook/src/<pkg>/`
(none for drawer/create pane). Bun tests `src/utils/*.test.ts`; `packageBoundary.test.ts` in both.

**Packages** — session-web (drawer) depends on session-create-web; session-create-web depends only on
client-web, react-commons, ui-web; prstack-web (PR-stack chat/screen) depends on chat, client,
session-create, ui; workspace-web owns projects screen.

### Exploration 2 — PR stacks: data, tools, RPCs, store

Queries: `grep -rn` (rs/ts/tsx/proto) for `\.stack\b`, `update_stack_atomic`,
`read_stack_with_resolved_branches`, `link_stack_node_to_child_session`, `sync_stack_node_from_child`,
`require_pr_stack_orchestrator`, `"spawn-child"`, `exec_tool_catalog`, `admitted_exec_tool_names`,
`StackOpJournal`, `stack_plan_json`, `parseStackPlan|PlannedPrRow|stackParents|groupSessionsByStack`,
`projects.yaml|projects_path_for_user`, `sweep|migrat`.

**Model (tddy-changeset, re-exported `tddy_core::changeset`)** — `changeset/stack.rs:16` `Stack {version,
nodes}`, `:25` `StackNode` (node_id, title, description, branch_suggestion, branch, session_id, parents,
pr_status, child_state, internal_status, display_order); methods `:73-246`; `:249 update_stack_atomic`
(read-modify-write, **no lock**, atomic rename only); `:260 link_stack_node_to_child_session`; `:283
resolve_stack_node_branch`; `:297 read_stack_with_resolved_branches`; `:314 sync_stack_node_from_child`;
`model.rs:90 pub stack: Option<Stack>`, `:95 orchestrator_session_id`, `:99 orchestrator_daemon_instance_id`;
`io.rs:13 read_changeset`.

**Production call sites (~60 in 22 files / 8 crates; ~110 more test lines in ~35 files)** —
tddy-pr-stack `stack_ops/{mod,nodes,repoint,order,adopt,pull_base,seed}.rs`, `assess.rs:306`;
tddy-workflow-recipes `pr_stack/{mod.rs:278,433,451, hooks.rs:166,215,281,332, bridge.rs:36}`,
`orchestrate_pr_stack/{bridge.rs:37,65,93,114,156,213, pr_actions.rs:19, hooks.rs:140}`;
tddy-tools `server.rs:1257,1275,1296,1305,1543,1628`; tddy-agent-launch `child_spawn_handler.rs:20`,
`svc_pr_status_for_caller.rs:120-124`; tddy-daemon-rpc `pr_stack/ports.rs:426,436,519,713`;
tddy-session-worktree `session_chain.rs:118 (parent_is_pr_stack_orchestrator: recipe=="pr-stack" ||
stack.is_some()),156,188,198`; tddy-session-activity `session_list_enrichment.rs:78`.

**`pr_*` tools (tddy-tools `server.rs:944-1107`, `#[tool]` on `PermissionServer`; by-name dispatch
`call_tool_by_name` 515-529)** — `orchestrator_dir()` :1185 = `TDDY_SESSION_DIR`; `TDDY_REPO_DIR` via
`repo_slug()` :1192 / `default_branch()` :1221; `real_gh()` :1215, token over `TDDY_SOCKET`
(`github_credential::github_token_for_session`); `refresh_internal_statuses` :1612 assumes
`sessions_root = dir.parent().parent()`. `pr_spawn_child` :1043 relays `{"type":"spawn-child"}` via
`dispatch_toolcall` → `StackChildSpawnHandler` (`tddy-agent-launch/src/child_spawn_handler.rs:38`),
attached only for recipe `pr-stack` (`svc_start_claude_cli_session.rs:39-60 managed_spawn_handlers`;
listener `tddy-cli-sessions/src/session_toolcall.rs:90-117`). No guard in tddy-tools.
Git per tool: merge/close/repoint are **GitHub API only** (`pr_actions.rs:36`, `patch_pr_base` :978);
`pr_resolve_conflicts` runs `git merge --no-commit` in an **agent-supplied `worktree_dir`** (:127,
`pr_actions.rs:76`); `pr_set_parents` → `realign_node_to_effective_base` (`stack_ops/repoint.rs:141`) →
`git_ops::rebase_onto` (`git_ops.rs:39`) does `git checkout <branch>` + rebase + force-push **in
`TDDY_REPO_DIR`** (fails when the branch is checked out in the child's worktree). Worktree-correct
pattern exists: `pull_base_into_node_branch` (`pull_base.rs:84`, `tddy_git::worktree_path_for_branch`)
and `tddy_worktree_service::session_git_ops` (project-home git tools, activity gate).

**Daemon exec-tool pattern (tddy-tool-engine)** — `catalog::ToolDef` (`catalog.rs:9`), `tool_catalog()`
:16; per-family module e.g. `project_home_git_tools.rs` (consts 30-36, `is_project_home_git_tool` 39,
`project_home_git_tool_defs` 53, `execute_project_home_git_tool` 152) over `ProjectHomePort`
(`project_home_tools.rs:98`, impl `tddy-daemon-rpc/src/exec_tool/project_home/port.rs`);
`project_home_catalogue.rs`: `SessionToolRole {ProjectHome, ConversationSpawner, Ordinary}`,
`session_offered_catalog_for` :131, `project_home_catalog_with` :153; host targeting
`project_home_target_host.rs`. Daemon `tddy-daemon-rpc/src/exec_tool/ports.rs:34 execute_tool`: peer
route → `authorize_exec_tool_caller` → hosted clone → `serve_project_home_for_caller_held_elsewhere` →
`resolve_exec_tool_worktree` → `reject_refused_local_exec_tool` (`role_guard.rs:21`) → `run_locally`
(`project_home.rs:90`); `ListExecTools` :199. Forward: `project_home_remote.rs:165 served_on_target_host`,
`:51 serve_project_home_for_caller_held_elsewhere` (owner check via peers' ListSessions), one hop;
generic `tddy-host-service/src/peer_routing.rs:165 rpc_served_by_peer`. Client:
`exec_tool_catalog()` `server.rs:1786`, `exec_tool_admission.rs` (`project_home_remote_tool_defs()` :58,
`is_advertised`), `dispatch_dynamic_tool` → `tddy-session-tool-client/src/lib.rs:376`.

**PrStackService (`tddy-service/proto/pr_stack.proto:16`)** — AddPlannedPr, GetPrStatus, RepointPlannedPr,
ReorderPlannedPr, PullBaseIntoBranch, QueryBranch, ResolveStackBase, LinkStackNode; all addressed by
orchestrator `session_id`, none by stack id; no Update/Delete/SetParents RPCs. Handlers: trait
`PrStackHandler` `tddy-pr-stack/src/rpc.rs`; impl `tddy-daemon-rpc/src/pr_stack.rs:24` +
`pr_stack/ports.rs` (808 lines). `SessionEntry.stack_plan_json = 23` (`session.proto:309`), filled by
`session_list_enrichment.rs:77,223,256,297`.

**Journal** — `tddy-workflow-recipes/src/orchestrate_pr_stack/transient.rs:17`,
`<orchestrator session>/.workflow/stack-op.json`. Per-node docs `<artifacts>/prs/<node_id>/`
(`tddy-pr-stack/src/docs.rs:76`); `stack-plan.yaml`; `stack-status.*`. All keyed to the orchestrator dir.

**Web** — `tddy-prstack-web/src/stackPlan.ts:70 parseStackPlan`, `PrStackScreen.tsx:195` (reads
`session.stackPlanJson`, overrides at 397/454/482), `PlannedPrList/Row*.tsx`, `orderStackNodes.ts`,
`startBlockers.ts`; routing `tddy-session-web/src/workflowViews.tsx:106` (recipe `pr-stack` →
PrStackScreen); `sessionStackGroups.ts:37` (by `orchestratorSessionId` only);
`tddy-session-create-web/src/utils/stackParents.ts` (`PR_STACK_RECIPES`). ~30 `PrStack*.cy.tsx` specs.

**Project store** — `tddy-projects/src/project_storage.rs`: `{data}/projects/projects.yaml`,
`ProjectData` :22 (`home_session_id?` :54), `set_project_home_session` :340. Path
`tddy-daemon-kernel/src/user_paths.rs:69 projects_path_for_user`. Per-project dir exists:
`tddy-session-activity/src/project_notification_log/storage.rs:5`
`<data>/projects/<project_id>/notifications.jsonl`, id guard `project_file()` :34; opened at
`tddy-daemon/src/runtime/mount_host.rs:80`. Startup sweep precedent `mount_host.rs:252
sweep_headless_files` (body in a library crate — tddy-daemon is wiring-only). `ProjectEntry`
(`project.proto:103`), `daemon_instance_id = 5`, `home_session_id = 11`.

**Recipe** — `pr_stack/mod.rs:265 next_goal_for_state_with_changeset` (Init + non-empty stack →
orchestrate); `:423 reseed_stack_from_plan_if_unspawned` (called from `hooks.rs:281`); `seed_and_recover`
(`bridge.rs:32`) reads `stack-plan.yaml` from the session root (possibly stale vs `artifacts/`).
Crate-shape tests: `tddy-workflow-recipes/tests/pr_stack_crate_shape.rs`,
`tddy-daemon-rpc/tests/rpc_handlers_shape.rs`; tddy-pr-stack must not depend on workflow-recipes or
session-lifecycle.

**"Has a stack" ≡ "is orchestrator" sites** — `parent_is_pr_stack_orchestrator`,
`next_goal_for_state_with_changeset`, `require_pr_stack_orchestrator` (recipe), `StackChildSpawnHandler`
gate, web `prStackOrchestrators`.

### Exploration 1 — project-home daemon code

No `.codegraph/`; grep/sed/Read. Queries: `EnsureProjectHomeSession|project_home|session_role|PROJECT_HOME_ROLE`,
`SessionStart|source.*clear|/clear`, `effort|thinking_level|reasoning_effort`, `^\s*rpc ` over protos,
tool-name consts in tddy-tool-engine, `TDDY_REMOTE_SESSION_ID|TOKEN`, `reap_idle_project_home_sessions`.

**Ensure** — `tddy-agent-launch/src/svc_ensure_project_home.rs`: entry :88-115 (peer route :95-101, then
global `ENSURE_LOCK` :56/:104, `set_up_main` vs `reach_recorded_main`); `caller_home_scope` :118-144;
`set_up_main` :170-197 (refuse → `apply_harness_profile` → type check → `home_worktree_for` →
`on_home_worktree` → `refuse_a_claude_cli_start_up_front` → `recorded_main` → `stop_previous_main`
:201-221 (TODO :209 unserved pid) → `start_and_record_main` :225-253 via `start_session_with_role` +
`record_main`); `recorded_main` :257-290; `reach_recorded_main` :148-165; `resume_stopped_home` :307-326.
Spec checks `project_home_start_spec.rs`: `refuse_what_main_cannot_honour` :24-50,
`refuse_a_type_main_cannot_run` :58-90 (`Capability::ProjectHomeMain`), `on_home_worktree` :95-118.
Home worktree `tddy-worktree-service/src/home_worktree.rs` (:35 path, :41-65 ensure, :70-77 ff).
`home_session_id` scalar in `tddy-projects/src/project_storage.rs:51-55`, setter :340-357;
`project.proto:131` field 11 (10 reserved). `EnsureProjectHomeSessionRequest` session.proto:376-392.

**Role** — refusal `tddy-agent-launch/src/harness_profile_request.rs:41-53,125-134`
(`refuse_a_requested_role`; all caller paths incl. exec StartSession); bypass `start_session_with_role`
:59-66 (pub(crate), only Ensure). Exec StartSession withholds role `tddy-tool-engine/src/start_session_args.rs:164-166`.
Role read `cli_branch_starts.rs:179-185`; written before spawn `claude_cli_spawn_steps.rs:97-117`
(called `opencode_cli_spawn.rs:274-282`, `jail_launch.rs:150`), final write `opencode_cli_spawn.rs:352-370`;
`refuse_a_role_the_agent_cannot_hold` `cli_agent_steps/conversation.rs:185-196`.
`PROJECT_HOME_ROLE` `tddy-changeset/src/session_metadata.rs:137`; `session_role` :127;
**`SessionMetadata` is `deny_unknown_fields` (:11)** — new fields break older daemons (tombstone
pattern :76-83). Proto: `StartSessionRequest.session_role = 43` (:689), last field `run_in_tmux = 47`
→ **next free 48**; `SessionEntry.session_role = 35`, last `reads_host_paths = 36`.
**Exact-string `PROJECT_HOME_ROLE` gates (6):** `project_home_launch.rs:255`, `role_guard.rs:44`,
`conversation_spawn.rs:25`, `project_home_idle.rs:87`, `project_home_remote.rs:146`, `conversation.rs:189`.

**Guards / catalogue** — `tddy-daemon-rpc/src/exec_tool/role_guard.rs:21-52`; `read_only.rs`
(`READ_ONLY_TOOLS` :12, `is_project_home_allowed_tool` :23, `project_home_allowed_tools()` :37);
`exec_tool/project_home.rs` `run_locally` :93-124, `DaemonProjectHome::execute` :284-307,
`project_home_port` :130-170 (`home_session_id = caller`), `offered_catalog` :186, `session_tool_catalog`
:202-227; caller gate `exec_tool/project_home/caller.rs:17-50`; role derivation
`tddy-agent-launch/src/conversation_spawn.rs:21-36` (`session_tool_role`: project-home → ProjectHome;
recipe grill-me → ConversationSpawner; else Ordinary). `project_home_catalogue.rs` `is_project_home_tool`
:37-61 (21 names), `SessionToolRole` :92-102, spawner sets :108-124.
Tool modules (tddy-tool-engine): `project_home_tools.rs` (ListProjectSessions etc; `ProjectHomePort` :98),
`start_session_args.rs`, `start_ledger.rs`, `start_options.rs`, `project_home_worktree_tools.rs`,
`session_follow.rs`, `project_home_terminate.rs`, `project_home_disk_refresh.rs`,
`project_home_terminal_tools.rs` (lease screen `project-home:{caller}` :41), `project_home_answer_tools.rs`,
`project_home_inbox_tools.rs`, `project_home_notify_tools.rs`, `project_home_git_tools.rs`. Daemon ports
`tddy-daemon-rpc/src/exec_tool/project_home/{port.rs,terminal.rs,answer.rs,notify.rs}`; `port.rs:53`
ListProjectSessions filters out caller (:69); StartSession records `orchestrator_session_id = caller`
(:103-119); TerminateSession :212-223. Target resolver refuses only the caller
(`tddy-worktree-service/src/session_target.rs:74`) → **another orchestrator on the shared home worktree
is a valid write target today**.
Cross-host: `project_home_remote.rs:54` owner check via peers' ListSessions, `:146` role must be
project-home (TODO :140-145), `served_on_target_host` :174-191.

**Launch (spec-driven)** — `tddy-coding-agent-specs/specs/claude.yaml:17-28` argv (`--model`,
`--resume {session_id}` / `--session-id {session_id}`), slots :37-43 (mcp, tool_allow, tool_deny,
append_system_prompt, hooks `claude-settings-local`), `conversation: { kind: daemon_session_id }` :47,
capabilities :48 (`project_home_main`, `requires_model`, `operator_notify`). Role wiring
`tddy-agent-launch/src/project_home_launch.rs` (allowed :61, denied :65, `spec_launch` :82-99, prompt
:72/:196/:199, env :178-184, token mint `main_plan` :247-265 gated on role :255 →
`tddy-session-split/src/agent_credentials.rs:35`); start `cli_agent_steps.rs:178,241-243`; resume
`:292,352` → `project_home_launch.rs:226-242`.

**Auto-stop** — `tddy-agent-launch/src/project_home_idle.rs:64`, filter :87 (role == project-home);
config `tddy-daemon-kernel/src/config/sections.rs:195-218`; schedule `tddy-daemon/src/runtime/tasks.rs:304-318`.

**Effort** — NOT FOUND (no proto/HarnessLaunch/claude.yaml field). Nearest `claude_args = 25` (jail
only; exec StartSession withholds). Model: `StartSessionRequest.model = 8`, profile fill
`harness_profile_request.rs:110-114`, curated `claude.yaml:57-65`, `harness_from_spec.rs:40-72`.

**Agent YAML** — coding-agent specs `tddy-coding-agent-specs/specs/*.yaml` + overrides
`<tddyhome>/tools/*.yaml` (`registry.rs:30`, `lib.rs:39-68,130`); `CodingAgentSection` `spec.rs:248-352`
(`deny_unknown_fields`; hooks `HooksSlot` :1104, resume `ResumeSlot` :795, capabilities, conversation
`ConversationSlot` `slots/conversation.rs:32-48` = DaemonSessionId | Minted | Worktree | ResumeToken);
`Capability` `capability.rs:15-39`. Harness profiles `<tddyhome>/agents/*.yaml`
(`tddy-discovery/src/harness_profile.rs:24-40`).

**Hooks / conversation id** — writer `slots/hooks.rs` (`ClaudeSettingsLocal` :38); builder
`tddy-agent-backend/src/claude_hooks.rs:84-133` (SessionStart, UserPromptSubmit, PostToolUse,
Notification, PermissionRequest, PreToolUse(AskUserQuestion), Stop, SessionEnd → `tddy-tools session-hook`);
`tddy-tools/src/session_hook.rs:144-205` → `ReportSessionStatusRequest` (activity.proto:265-286; no
conversation-id/source field); stdin `tddy-changeset/src/session_activity.rs:137-158` (`HookEvent` parses
Claude `session_id` but never forwards; **`source` not parsed**); daemon
`tddy-session-activity/src/service/hook_reports.rs:159-222`. **/clear handling NOT FOUND.** Claude's
conversation id == tddy session id (`cli_agent_steps/conversation.rs:85-86,124`) → after `/clear` a
resume brings back the pre-clear conversation. Other kinds: `cursor_chat_id` :42, `harness_resume_token`
:47 (`cli_resume_token.rs`, `warp_resume_token.rs`). Transcript readers key on session id
(`claude/transcript_usage.rs:37`).

**Session control coverage** — ResumeSession RPC (session.proto:38, `svc_resume_session.rs:34`): no exec
tool. DeleteSession RPC (:48, `session_deletion.rs`): no exec tool. RemoveWorktree/CleanWorktree/
RestoreSessionWorktree RPCs (worktree.proto:13,24,25): no exec tool. Send a turn to a running session:
only `SendSessionKeys` (+claim); `PromptAgentConversation` is roster-only. Stop: `TerminateSession` exists.
Pause/status marker: NOT FOUND.

## Exploration 2 — pass 2 contract (6c–6f, 2026-11-12/13)

- **The mark field ripples.** `SessionMetadata` has no `Default` and is built by struct literal at
  ≈56 sites in 9 crates (mostly tests); each got `orchestration_mark: None`, found by
  `cargo check --all-targets` rather than grep — the `tmux_session` precedent (#814, 55 sites).
- **Restore is session-addressed.** `RestoreSessionWorktree` (`tddy-worktree-service/src/service.rs:500`)
  takes a `session_id` and re-cuts the worktree from the session's changeset. A deleted worktree is
  not in `ListWorktrees`, so "by path as ListWorktrees lists it" cannot hold for Restore; the
  contract first resolved `path` to the session that recorded it; the developer chose
  `session_id` instead (2026-11-13). The home worktree is the project's home path,
  `home_worktree_path(worktree_dir(main_repo), project_id)`, whoever calls (developer, 2026-11-13).
- **Web Delete terminates, it does not refuse.** `delete_session_directory` (`session_deletion.rs:246`)
  kills a live pid; `DeleteSession`'s refusal of a running session is the tool's own check.
- **n4's turn tools are unwired** (not in `is_project_home_tool`, the target list, the catalogue or
  the dispatch); n5 follows suit, so every acceptance call today falls through
  `ExecToolRpcHandler::run_locally` to the local runner and is refused there.
- **Closed-world lists** green must extend: `tddy-tools/src/exec_tool_admission.rs`,
  `tddy-tool-engine/src/read_only.rs` (the `mcp__tddy-tools__*` list),
  `tddy-daemon-rpc/tests/project_home_session_tools_acceptance.rs`.
- **Pausing reuses Terminate.** The tool records the mark, then calls the existing
  `ProjectHomePort::terminate_session` for an active target; no new stop path.
