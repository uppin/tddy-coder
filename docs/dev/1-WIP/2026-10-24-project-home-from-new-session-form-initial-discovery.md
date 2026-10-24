# Initial Discovery: main is set up with the New-session form

**Changeset**: [2026-10-24-project-home-from-new-session-form.md](./2026-10-24-project-home-from-new-session-form.md)
**Date**: 2026-10-24
**Passes**: 2

## Combined Conclusions

**The reported bug.** The setup pane and the Projects card offer every `ListAgents` row as a home
agent, and `SetProjectHomeAgent` accepts any listed id (`checked_home_agent`,
`tddy-daemon-rpc/src/project/home_agent_handler.rs`). `EnsureProjectHomeSession` then resolves the
stored id **only** against harness profiles in `<tddyhome>/agents/*.yaml` whose harness is
`claude-cli` (`home_agent_profile`, `tddy-agent-launch/src/svc_ensure_project_home.rs:170-194`). The
catalog's allowlist rows (`claude` → "Claude (opus)") can be stored but can never start:
`[failed_precondition] home agent "claude" is not a harness profile`. Two vocabularies, one check
each.

**State A — daemon.**
- `EnsureProjectHomeSessionRequest {session_token=1, project_id=2, daemon_instance_id=3}`; response
  `{SessionEntry session=1, string outcome=2}` (`started|resumed|running`).
- Ensure builds the whole `StartSessionRequest` itself: `session_type: "claude-cli"`, model from the
  profile, `managed_codebase: true`, `session_role: "project-home"`, `repo_path:` the home worktree,
  provider account through `request_on_profile` when the profile names one. No initial prompt, no
  permission mode (→ `auto`), no sandbox. Calls `start_session_with_role` (the only path that
  skips `refuse_a_requested_role`).
- The home session is **found by role**: `find_home_session` = first session with
  `project_id == p && session_role == "project-home"`. Nothing is stored on the project.
- Running = pid alive or terminal served; stopped → fast-forward the home worktree, resume.
- Ensures are serialized by a process-wide `ENSURE_LOCK`.
- Peer routing: `rpc_served_by_peer(SESSION_SERVICE, "EnsureProjectHomeSession", …)`.

**Which session types can hold main read-only.** Only `claude-cli`:
- `cursor-cli` accepts `managed_codebase` and discards it (`cursor_cli_spawn.rs` `let _ = managed_codebase;`) —
  [TODO](../todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md).
- `opencode-cli` / `muse-cli` refuse it (`start_cli_harness`: "managed_codebase is not supported for {} sessions").
- `tool` ignores it (no `managed_codebase` on the tool spawn path).
- Sandboxed `claude-cli` (`start_sandboxed_claude_cli_from_request`) passes `managed_codebase` and
  `repo_path` but **drops `session_role`** — so a sandboxed main would not be recorded as
  project-home. Only `start_claude_cli_from_request` carries the role.

**State A — storage / project RPC.** `ProjectData.home_agent: Option<String>` (serde default, skip
when none; no `deny_unknown_fields`, so a removed key in an older row is ignored on read).
`ProjectEntry.home_agent = 10`. `SetProjectHomeAgent` chain: proto → `tddy-projects`
handler/service/lib → `tddy-daemon-rpc` `ports.rs` → `home_agent_handler.rs` → peer forward
`forward_set_project_home_agent` (`tddy-daemon-kernel/src/peer_calls.rs`) →
`project_storage::set_project_home_agent`. `RESIDUAL_METHODS` in
`tddy-service/tests/unbundle_service_split.rs` lists both RPCs. ~60 `ProjectData { … home_agent: None }`
struct literals across the workspace (list in Exploration 1).

**State A — web.**
- `CreateSessionPane.tsx` (483 lines, budget 500) is the New-session form. Five types
  (`CreateSessionTypeToggle`: tool, claude-cli, cursor-cli, opencode-cli, muse-cli), no disabled
  state. No props to fix the project/host, hide sections, disable types with a reason, or hand the
  built request to the caller — submit always `StreamStartSession`s. `buildStartSessionRequest`
  (`createSessionRequest.ts`) is pure and already unit-tested: the natural seam.
- `ProjectHomeSetupPane.tsx` renders `ProjectHomeAgent` (select) + **Start main session**.
- `ProjectHomeEntry.tsx` / `projectHomeGroups.ts` gate "configured" on `homeAgentLabel !== null`
  and find the home session by `sessionRole === "project-home"`.
- `SessionsDrawerScreen.tsx` (490 lines) wires `useProjectHomeSetup`, `useEnsureProjectHome`,
  `useSessionsDrawerProjectHomes`. Ensure selects with `handleSelectSession` (navigate only).
- Projects screen: `ProjectHomeAgent` on `ProjectCard`, `#/projects?project=<id>` focus exists only
  for that picker.

**Constraints for State B.**
- Read-only stays: only `claude-cli` may start main; the daemon refuses any other type itself,
  the web shows the others disabled with the reason.
- Sandbox placement must be refused for main until the sandboxed claude-cli path records the role
  (else a main runs that the daemon reads as ordinary — the one thing the role exists to prevent).
- `CreateSessionPane.tsx` and `SessionsDrawerScreen.tsx` are both within ~20 lines of the 500 budget:
  the variant must live in new modules, the pane grows only by wiring.
- No claimed code issues in the touched packages. TODO hits: see changeset `## Prerequisites`.

## Exploration 1: project-home + New-session form — 2026-10-24

**Agent**: Explore subagent
**Scope**: proto, daemon ensure / SetProjectHomeAgent chain, StartSession managed-mode support per
type, web New-session form, drawer wiring, Projects screen, tests, docs.

### Sequence

1. Grep `home_agent|HomeAgent|homeAgent|EnsureProjectHome|ensure_project_home|ProjectHomeSetup|projectHomeSetup|home_session_id|homeSessionId|project-home|projectHome` repo-wide (excl. node_modules/target/.git) — footprint.
2. Grep proto files `session.proto`, `project.proto`, `exec_tools.proto`; Read `session.proto` 20–49, 340–379, 450–659; `project.proto` 10–175.
3. Grep generated TS `packages/tddy-web/src/gen/*_pb.ts`; `tddy-service/tests/unbundle_service_split.rs`.
4. Read `tddy-agent-launch/src/svc_ensure_project_home.rs` (full), `harness_profile_request.rs` (full).
5. Grep `start_session_with_role|session_role|PROJECT_HOME_ROLE|ensure_project_home_session_at_session_coordinate` in Rust src.
6. Read `svc_start_session_core.rs`, `cli_branch_starts.rs`, `start_request_checks.rs`; grep `managed_codebase` in `placement.rs`, `cursor_cli_spawn.rs`, `svc_start_claude_cli_session.rs`; Read `cursor_cli_spawn.rs` 300–345, `svc_start_claude_cli_session.rs` 40–175.
7. Read `harness_profile_start.rs` 1–80; `tddy-discovery/src/harness_profile.rs`.
8. Read `tddy-daemon-rpc/src/project/home_agent_handler.rs`; grep the handler chain (`ports.rs`, `project.rs`, `coordinate_handlers.rs`, `entries.rs`, `test_util.rs`, `peer_calls.rs`, `tddy-projects/src/*.rs`, `livekit_peer_discovery.rs`).
9. Read `project_storage.rs` 1–70, 290–370; `peer_calls.rs` 200–230; `entries.rs` 60–100.
10. Grep kernel config, `tddy-daemon/src/runtime/tasks.rs`, `mount_host.rs` for project-home.
11. Web: list `src/components/sessions`; Read `CreateSessionPane.tsx`, `createSessionPaneProps.ts`, `createSessionRequest.ts`, `CreateSessionTypeToggle.tsx`, `useCreateSessionSubmit.ts`, `CreateSessionDialog.tsx`, `ProjectHomeSetupPane.tsx`, `useProjectHomeSetup.ts`, `useEnsureProjectHome.ts`, `ProjectHomeEntry.tsx`, `useSessionsDrawerProjectHomes.ts`, `utils/projectHomeGroups.ts`, `SessionsDrawerScreen.tsx`; parts of `SessionMainPane.tsx`, `SessionDrawer.tsx`, `routing/appRoutes.ts`.
12. Read `ProjectHomeAgent.tsx`; grep/read `ProjectsAppPage.tsx`, `ProjectCard.tsx`, `ProjectsScreen.tsx`.
13. Web tests: grep cypress + src specs/stories; Read `ProjectHomeSetupPaneAcceptance.cy.tsx`, `SessionsDrawerProjectHomeAcceptance.cy.tsx`, `ProjectHomeSetupLinkAcceptance.cy.tsx`, `ProjectsScreenHomeAgentAcceptance.cy.tsx`; grep `testIds.ts`, `projectsScreenPage.ts`; Read `cypress/support/rpc/inMemory.tsx`, `daemonSessionHostBackend.ts` 130–420, `cypress/support/component.ts` 30–90, `connectionRpcs.ts`, `CreateSessionPane.cy.tsx` 1–80, `projectHomeGroups.test.ts`.
14. Rust tests: outline/read `ensure_project_home_session_acceptance.rs` 120–416, outline `set_project_home_agent_acceptance.rs`, `project_home_agent_storage.rs`, `project_home_idle_stop_acceptance.rs`, the three `project_home_*_tools` tests; grep `home_agent` in Rust.
15. Read `svc_session_lifecycle_ports.rs` 75–90, 150–165; `test_util.rs` 220–232; `unbundle_service_split.rs` 550–580.
16. Docs: grep headings in the three feature docs; read `project-concept.md` 57–70, 111–159; `session-drawer.md` 76–107, 487–500; `projects-screen-multi-host.md` 88–104.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | footprint pattern (step 1) | repo | `svc_ensure_project_home.rs` 19, `home_agent_handler.rs` 20, `project_storage.rs` 12, `ProjectHomeSetupPaneAcceptance.cy.tsx` 37, `testIds.ts` 29, `set_project_home_agent_acceptance.rs` 22, `project_pb.ts`/`session_pb.ts` 22 each, `project-concept.md` 18, `ProjectsScreenHomeAgentAcceptance.cy.tsx` 18, `SessionsDrawerProjectHomeAcceptance.cy.tsx` 17, `SessionsDrawerScreen.tsx` 15, `ensure_project_home_session_acceptance.rs` 14, `project_home_agent_storage.rs` 14, `ProjectCard.tsx` 12, `session-drawer.md` 11 |
| Grep | `home_session_id\|homeSessionId` | repo | none |
| Grep | `home_agent: None` | Rust | tddy-projects src `project_storage.rs` 413/646/677/716, `project_provision.rs` 109/190/243; tests `project_accounts_storage.rs:31`, `project_local_gh_storage.rs:20`, `project_default_branch_resolution_acceptance.rs` 96/133/164/190, `project_storage_add_or_get.rs:22`; tddy-daemon-rpc src `coordinate_handlers.rs` 170/328 + 16 test files; tddy-agent-launch `start_request_checks_unit_tests.rs:24`; tddy-daemon tests `multi_host_acceptance.rs` 136/284, `plan_dialog_acceptance.rs:131`, `local_token_uds.rs:362`, `session_agent_remote_acceptance.rs:628`, `code_navigation_acceptance.rs:110`, `support/mod.rs:128`; tddy-session-lifecycle 7 tests + 2 unit-test modules; tddy-session-files `host_documents.rs:614`; tddy-session-activity `remote_git_pack_execution.rs` 166/216; tddy-telegram-control 6 sites + `workflow_spawn.rs:503`; tddy-worktree-service 9 test sites |
| Grep | `home_agent: String::new()` | Rust | `tddy-daemon-livekit/src/livekit_peer_discovery.rs:2073`; `tddy-daemon-rpc/tests/non_blocking_peer_fanout_acceptance.rs` 118/183, `list_projects_local_only.rs:50`, `list_projects_multi_daemon_aggregation.rs` 103/115 |
| Grep | `managed_codebase` | `tool_session_spawn.rs`, `tool_spawn_plan.rs` | none — tool path ignores it |
| Grep | `streamStartSession` | cypress support | `cypress/support/component.ts:53` (`registerServerStreamFallback`) |

### Inspected files

#### `packages/tddy-service/proto/session.proto`

**Why**: Ensure and StartSession wire shapes.
**Excerpt**:

```proto
message EnsureProjectHomeSessionRequest { string session_token = 1; string project_id = 2; string daemon_instance_id = 3; }
message EnsureProjectHomeSessionResponse { SessionEntry session = 1; string outcome = 2; } // "started"|"resumed"|"running"
// StartSessionRequest (450–647): agent=4, session_type=7, model=8, initial_prompt=13, permission_mode=14,
// sandbox=16, managed_codebase=17, repo_path=24, dangerously_skip_permissions=26,
// codebase_daemon_instance_id=32, sandboxed_codebase=39, provider_account_id=41,
// provider_built_in=42, session_role=43 ("set only by EnsureProjectHomeSession; StartSession refuses it")
```

#### `packages/tddy-service/proto/project.proto`

**Why**: home-agent wire shape.
**Excerpt**:

```proto
rpc SetProjectHomeAgent(SetProjectHomeAgentRequest) returns (SetProjectHomeAgentResponse);   // :35
string home_agent = 10;                                                                     // ProjectEntry :130-132
message SetProjectHomeAgentRequest { string session_token = 1; string project_id = 2; string home_agent = 3; string daemon_instance_id = 4; }
message SetProjectHomeAgentResponse { ProjectEntry project = 1; }
```

#### `packages/tddy-agent-launch/src/svc_ensure_project_home.rs`

**Why**: the whole ensure decision.
**Excerpt**:

```rust
const ENFORCING_HARNESS: &str = "claude-cli";
static ENSURE_LOCK: tokio::sync::Mutex<()> = …;

// home_agent_profile (170–194)
let profile = load_harness_profiles(&self.tddy_data_dir.join("agents"))
    .into_iter().find(|profile| profile.name == agent)
    .ok_or_else(|| Status::failed_precondition(format!(
        "home agent {agent:?} is not a harness profile, so it names no harness to run")))?;
if profile.harness != ENFORCING_HARNESS { return Err(Status::failed_precondition(…)) }

// start_home (202–238)
let start = StartSessionRequest { session_token, project_id, session_type: ENFORCING_HARNESS,
    model: profile.model.clone(), managed_codebase: true, session_role: PROJECT_HOME_ROLE,
    repo_path: home, ..Default::default() };
let start = if profile.provider_account.trim().is_empty() { start } else {
    request_on_profile(start, profile, &self.host.session_account_access(session_token))? };
self.start_session_with_role(start, &AttachmentProgressSink::discarding()).await?

// find_home_session (311–320)
list_sessions_in_dir(sessions_base)?.into_iter()
    .find(|s| s.project_id == project_id && s.session_role == PROJECT_HOME_ROLE)
```

#### `packages/tddy-agent-launch/src/harness_profile_request.rs`

**Why**: how a caller start differs from the daemon's role start.
**Excerpt**:

```rust
// start_session_core (28–35): refuse_a_requested_role(&req)?; self.start_session_with_role(req, sink)
// start_session_with_role (39–46, pub(crate)): apply_harness_profile(req) then start_session_on_request
// request_on_profile (70–101): profile_start → choose_route → access.provider_account(&chosen)
//   → StartSessionRequest { session_type, agent, model (request's if set), provider_account_id, ..req }
// refuse_a_requested_role (104–113): non-empty session_role → PERMISSION_DENIED
```

#### `packages/tddy-agent-launch/src/cli_branch_starts.rs`, `cursor_cli_spawn.rs`, `placement.rs`

**Why**: which types can be held to managed mode.
**Excerpt**:

```rust
// cli_branch_starts.rs:125-126 — start_claude_cli_from_request is the only path passing the role
session_role: req.session_role.trim(), repo_path: req.repo_path.trim(),
// start_sandboxed_claude_cli_from_request (27–63): repo_path + managed_codebase, NO session_role
// start_cli_harness (252–278):
if req.managed_codebase { return Err(Status::invalid_argument(format!(
    "managed_codebase is not supported for {} sessions", …))) }
// cursor_cli_spawn.rs:312-332 prepare_cursor_cli_launch
let _ = managed_codebase;
// placement.rs:86-89,120-143: sandboxed_codebase / codebase_daemon_instance_id refused unless claude-cli
```

#### `packages/tddy-agent-launch/src/svc_start_session_core.rs`

**Why**: StartSession dispatch.
**Excerpt**: model required for `claude-cli` and `cursor-cli` (141–150); dispatch `"workspace"` (225),
`"claude-cli"` (348: `cli_start_prelude`, `managed_recipe_for`, sandboxed or
`start_claude_cli_from_request`), `"cursor-cli"` (387), other `cli_harness` (425), else tool session.

#### `packages/tddy-daemon-rpc/src/project/home_agent_handler.rs`

**Why**: SetProjectHomeAgent validation.
**Excerpt**:

```rust
let listed = listed_agents(&self.config, &self.tddy_data_dir, self.model_registry.as_deref()).await?;
if listed.iter().any(|agent| agent.id == home_agent) { return Ok(Some(home_agent)); }
Err(Status::invalid_argument(format!("home agent {home_agent} is not listed by this daemon's agent catalog")))
```

#### `packages/tddy-daemon-rpc/src/catalog/agent_rows.rs`

**Why**: what "listed" means.
**Excerpt**:

```rust
Ok(agent_allowlist_rows(config, &assistants).into_iter()
    .map(|row| (row.id, row.display_label))
    .chain(harness_profile_rows(&profiles))   // allowlist rows AND harness profiles
    .map(|(id, label)| CatalogAgentInfo { id, label }).collect())
```

#### `packages/tddy-projects/src/project_storage.rs`

**Why**: storage shape.
**Excerpt**:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectData { …, #[serde(default, skip_serializing_if = "is_false")] pub local_gh: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub home_agent: Option<String>, }
// set_project_home_agent(projects_dir, project_id, Option<&str>) (341–357): read, find, set, write
// set_project_accounts (≈300–338): same shape
```

#### `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` + props/request/toggle/submit

**Why**: reuse target.
**Excerpt**:

```ts
// createSessionRequest.ts:7
export type SessionType = "tool" | "claude-cli" | "cursor-cli" | "opencode-cli" | "muse-cli";
// buildStartSessionRequest(form, branchOverrides, attachments) (63–237), claude-cli branch 199–236:
//   provider fields, permissionMode, dangerouslySkipPermissions, initialPrompt, sandbox,
//   managedCodebase, sandboxedCodebase, specializedAgents, semanticIndex, codebaseDaemonInstanceId, sshConfigHost
// useCreateSessionSubmit.ts: isSubmitEnabled — tool needs projectId&&agent&&toolPath&&model, others projectId&&model;
//   submitCreation → startSessionStreamed → onCreated(res.sessionId)
// CreateSessionTypeToggle.tsx: SESSION_TYPE_OPTIONS tool/claude-cli/cursor-cli/opencode-cli/muse-cli,
//   data-testid `create-session-type-${id}`, aria-pressed, no disabled state
// CreateSessionInitialValues: sessionType, projectId, recipe, model, permissionMode, …, initialPrompt,
//   daemonInstanceId, attachments — no agent, no providerAccountId
```

#### `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx` and project-home hooks

**Why**: wiring to replace.
**Excerpt**:

```tsx
// 116: useSessionsDrawerProjectHomes(catalogClient, projects) → {homes, agents}
// 117–123: useProjectHomeSetup({projectClient, sessionToken, selectedInstanceId, onStored: reloadProjects})
// 256–260: useEnsureProjectHome(client, sessionToken, handleSelectSession)
// 389–397:
<ProjectHomeSetupPane projectId={setupProjectId} project={projects.find(…) ?? null} agents={agents}
  onSetHomeAgent={(agentId) => projectHomeSetup.setHomeAgent(setupProjectId, agentId)}
  onStart={() => ensureProjectHome(setupProjectId, selectedInstanceId ?? "")} onClose={projectHomeSetup.close} />
// utils/projectHomeGroups.ts: ProjectHomeInfo {projectId, mainBranchRef, homeAgentLabel: string|null};
//   projectHomeSlot finds by sessionRole === "project-home"; withProjectHomeGroups adds groups only when homeAgentLabel !== null
```

#### `packages/tddy-web/src/components/projects/ProjectHomeAgent.tsx`, `ProjectCard.tsx`, `ProjectsScreen.tsx`, `ProjectsAppPage.tsx`

**Why**: picker to remove.
**Excerpt**: `ProjectCard.tsx:181-193` renders `<ProjectHomeAgent … daemonInstanceId: primaryHost>`;
`ProjectsAppPage.tsx` `useProjectsRpc.setProjectHomeAgent` (115–121), `useListedAgents` (157–179),
`focusedProjectId = location.params[PARAM_PROJECT]` (261).

#### Tests

- `packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` (416 lines): fixture
  `a_daemon_with_a_project(home_agent: Option<&str>)` (185–239) writes `projects.yaml` with an optional
  `home_agent:`, agents `home-claude.yaml` (claude-cli) and `home-cursor.yaml` (cursor-cli), stub
  `claude` echoing argv; tests: starts managed on home worktree at main ref; running returns same;
  stopped fast-forwards + resumes; no home agent refused; cursor-cli refused; listed with role;
  pre-approved project-home tool set; StartSession naming a role refused.
- `project_home_idle_stop_acceptance.rs`: same fixture with `(home_agent, idle_stop_secs)`.
- `tddy-daemon-rpc/tests/set_project_home_agent_acceptance.rs`, `tddy-projects/tests/project_home_agent_storage.rs`.
- Role-seeded (no home_agent dependency): `project_home_session_tools_acceptance.rs`,
  `project_home_git_tools_acceptance.rs`, `project_home_read_only_exec_tools_acceptance.rs`.
- Web: `ProjectHomeSetupPaneAcceptance.cy.tsx` (7), `SessionsDrawerProjectHomeAcceptance.cy.tsx` (7),
  `ProjectHomeSetupLinkAcceptance.cy.tsx` (4), `ProjectsScreenHomeAgentAcceptance.cy.tsx` (2),
  `src/utils/projectHomeGroups.test.ts`, `CreateSessionRequestBuilder.cy.ts`, ~30 `CreateSession*` specs.
- Helpers: `cypress/support/rpc/inMemory.tsx` `mountWithRpc`; `anInMemoryRpcBackend` (`.onUnary`,
  `.implement`, `.callsTo`); `aSessionServiceBackend(scenario)` has `startSession` but not
  `ensureProjectHomeSession`; `ProjectService` only `listProjects`/`listProjectBranches`;
  `registerServerStreamFallback` serves `streamStartSession` from a unary stub.

### Findings

1. Home session resolved by role, not id; a `home_session_id` is a new `ProjectData` field + a new
   `ProjectEntry` field (next free 11) + `entries.rs:77` mapping + the ~60 literals.
2. Ensure owns the whole start spec today; accepting the form's means an embedded start request
   the daemon overrides for `project_id`, `repo_path`, `managed_codebase`, `session_role`,
   branch fields and placement.
3. Only `claude-cli` enforces managed mode end to end; the sandboxed claude-cli path drops the role.
4. The form has no fixed-project / hidden-section / disabled-type / caller-submit seams;
   `buildStartSessionRequest` is the seam.
5. Selecting after ensure uses `handleSelectSession`, not `handleSessionCreated`'s attach path.
6. Signal/delete already exist (`useSessionsDrawerPaneActions`, `SignalSession`/`DeleteSession`).
7. Removing `SetProjectHomeAgent` touches proto, both gen dirs, tddy-projects, tddy-daemon-rpc,
   peer_calls, `RESIDUAL_METHODS`, the Projects screen, drawer hooks, testIds, page objects, four
   Cypress specs and three doc sections.

## Exploration 2: the refusal and the cross-check — 2026-10-24

**Agent**: parent Grep/Read
**Scope**: reproduce the reported error to code; Step 2b scans; file budgets.

### Sequence

1. Grep `is not a harness profile` in `packages` — locate the refusal.
2. Read `packages/tddy-web/src/components/sessions/ProjectHomeSetupPane.tsx`, `packages/tddy-web/src/components/projects/ProjectHomeAgent.tsx` — what the pane offers.
3. Read `svc_ensure_project_home.rs` 100–260 — the refusal and start.
4. Read `tddy-discovery/src/harness_profile.rs` 1–80; `home_agent_handler.rs` 1–120; `SessionsDrawerScreen.tsx` 370–410.
5. Read `tddy-daemon-rpc/src/catalog/agent_rows.rs` `listed_agents`.
6. Read `docs/ft/web/projects-screen-multi-host.md` 80–115, `docs/ft/daemon/project-concept.md` 105–160, `docs/ft/web/session-drawer.md` 490–530, `docs/dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`.
7. `ls packages/{tddy-agent-launch,tddy-daemon-rpc,tddy-projects,tddy-web,tddy-service,tddy-discovery,tddy-session-lifecycle}/docs/code-issues/`; grep `Claimed by:` there — none claimed.
8. Grep `project.home|home.agent|home session|create.?session.?form|new.session` in `docs/dev/todo/`; Read the four project-home/drawer hits.
9. Read `project_storage.rs` 15–60 (no `deny_unknown_fields`); `codebasePlacement.ts` 1–60; `CreateSessionTypeToggle.tsx`; `wc -l` on `CreateSessionPane.tsx` (483), `SessionsDrawerScreen.tsx` (490), `svc_ensure_project_home.rs` (320).

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `is not a harness profile` | `packages` | `tddy-agent-launch/src/svc_ensure_project_home.rs:183` |
| Grep | `Claimed by:` | the seven packages' `docs/code-issues/` | none |
| Grep | project-home terms | `docs/dev/todo/` | `2026-10-23-project-home-running-heuristic.md`, `2026-10-23-project-home-git-push-remote-and-token.md`, `2026-10-23-project-home-auto-stop-untracked-processes.md`, `2026-10-23-session-drawer-grew-past-500-by-project-groups.md` (+ unrelated) |

### Inspected files

#### `packages/tddy-web/src/components/sessions/codebasePlacement.ts`

**Why**: is there an existing home for "why this choice is unavailable" rules?
**Excerpt**:

```ts
export type CodebasePlacementChoice = "none" | "sandbox" | "managed" | "sandboxedCodebase";
// "Why `host` cannot serve the sandboxed-codebase placement, or `null` when it can." — a reason-or-null
// rule, tested exhaustively in CodebasePlacementRules.cy.ts; the pane grows only by the control.
```

#### `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`

**Why**: `coordinate_handlers.rs` holds two `home_agent: None` literals.
**Excerpt**: "586 production lines … budget 500 … unclaimed". Removing a field shrinks it; no growth.

### Findings

- The refusal is reproduced to code; root cause is the two vocabularies (Combined Conclusions).
- A reason-or-null rule module (`codebasePlacement.ts` pattern) is the house shape for the disabled-type reasons.
- `ProjectData` tolerates a stale `home_agent:` key on read.
