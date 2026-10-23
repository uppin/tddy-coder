# Changeset: Project-home session

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto field + RPC, session lifecycle orchestration (start/resume/running decision), a new worktree kind, server-side tool refusal, across tddy-service, tddy-changeset, tddy-session-activity, tddy-agent-launch, tddy-worktree-service, tddy-tool-engine, tddy-daemon-rpc, tddy-web
**Stack**: #project-home 4/7. Base: `feature/project-home/activity-sidebar`.
**PR**: [#699](https://github.com/uppin/tddy-coder/pull/699)

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-22-project-home-session-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [`2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`](../todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md) | ⚠ DURING | The read-only guarantee needs an enforceable harness. Ensure refuses a home agent whose harness cannot enforce managed mode, naming it. Recorded, not fixed. |
| [`packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md) | ⚠ DURING | Ensure builds a `StartSessionRequest` and calls the existing start; it adds no branch to `start_session_core`. |
| [`packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-session-resume-session-at-session-coordinate.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-session-resume-session-at-session-coordinate.md) | ⚠ DURING | Resume is called, not modified. |
| [`packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md) | ⚠ DURING | The role filter lives in a new `read_only.rs`; `lib.rs` gains only the call. |
| [`packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md`](../../../packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md) | ⚠ DURING | Home worktree in a new `home_worktree.rs`. |
| [`packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md`](../../../packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md) | — | Not touched by this node. |

## Responsibility

- `SessionEntry.session_role` / `SessionMetadata.session_role` and their listing.
- `SessionService.EnsureProjectHomeSession`: the started / resumed / running decision, both refusals,
  and the session it starts (managed, home agent, role `project-home`).
- The **home worktree**: create detached at the fetched `main_branch_ref`; fast-forward before resume.
- The **read-only role filter**: which exec tools a `project-home` session may see and run, enforced in
  advertisement and in `ExecuteTool`.
- The drawer's pinned **main** entry per project group and its click → ensure → select flow.

## Boundaries

- No idle auto-stop (node 6). No session/worktree/git tools for the agent (nodes 5, 7).
- Does not change start/resume for any other session, nor `start_session_core`'s branches.
- Does not change the home-agent setting or its validation (node 1), the group component's grouping
  rule (node 2), or `managed_codebase` recording / the sidebar default (node 3).
- Does not fan the home session out to other hosts — one per project per host.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/project-home/home-agent-setting` (1/7) | `ProjectData.home_agent`, `ProjectEntry.home_agent` | ensure reads it to pick the agent; the pinned entry shows its label / hint | change the field, setter or validation |
| `feature/project-home/project-groups` (2/7) | `groupSessionsByProject`, `SessionProjectGroup` | renders the pinned entry as the group's first child; filters the home session out of the group's rows | change grouping, ordering or collapse |
| `feature/project-home/activity-sidebar` (3/7) | `managed_codebase` recorded + listed; `defaultActivitySidebarOpen` | the home session is managed, so its sidebar opens by default — asserted, not reimplemented | add a role case to `defaultActivitySidebarOpen` or touch the sidebar |

## Draft PR contract

```proto
// session.proto
message SessionEntry { /* … */ string session_role = 35; }
rpc EnsureProjectHomeSession(EnsureProjectHomeSessionRequest) returns (EnsureProjectHomeSessionResponse);
message EnsureProjectHomeSessionRequest { string session_token = 1; string project_id = 2; string daemon_instance_id = 3; }
message EnsureProjectHomeSessionResponse { SessionEntry session = 1; string outcome = 2; } // "started" | "resumed" | "running"
```
```rust
// tddy-changeset::session_metadata
pub const PROJECT_HOME_ROLE: &str = "project-home";
pub struct SessionMetadata { /* … */ #[serde(default, skip_serializing_if = "Option::is_none")] pub session_role: Option<String> }
// tddy-worktree-service::home_worktree (new)
pub fn ensure_home_worktree(main_repo: &Path, worktrees_root: &Path, project_id: &str, main_branch_ref: &str) -> Result<PathBuf, HomeWorktreeError>;
pub fn fast_forward_home_worktree(home: &Path, main_branch_ref: &str) -> Result<(), HomeWorktreeError>;
// tddy-tool-engine::read_only (new)
pub fn is_read_only_tool(name: &str) -> bool;
pub fn read_only_catalog() -> Vec<ToolDef>;
// tddy-worktree-service::home_worktree — also pub fn home_worktree_path(worktrees_root, project_id) -> PathBuf
// tddy-agent-launch::svc_ensure_project_home (new)
pub enum EnsureOutcome { Started, Resumed, Running }   // .as_wire() -> "started" | "resumed" | "running"
impl LaunchSessions { pub async fn ensure_project_home_session_at_session_coordinate(&self, Request<EnsureProjectHomeSessionRequest>) -> Result<Response<EnsureProjectHomeSessionResponse>, Status> }
// tddy-session-lifecycle: SessionHandler::ensure_project_home_session, wired through SessionServiceImpl,
// DaemonSessionHost and TestDaemon; `EnsureProjectHomeSession` joins RESIDUAL_METHODS (unbundle_service_split)
```
```ts
// packages/tddy-web/src/components/sessions/ProjectHomeEntry.tsx (new)
export function ProjectHomeEntry(props: { home: ProjectHomeInfo; homeSession: SessionEntry | null; selected: boolean; onEnsure(projectId: string): Promise<void> }): ReactElement | null;
// SessionDrawerProps gains (optional): projectHomes?: ReadonlyArray<ProjectHomeInfo>; onEnsureProjectHome?(projectId): Promise<void>
export interface ProjectHomeInfo { projectId: string; mainBranchRef: string; homeAgentLabel: string | null }
// packages/tddy-web/src/components/sessions/useEnsureProjectHome.ts (new)
export function useEnsureProjectHome(client: Client<typeof SessionService> | null, sessionToken: string, onEnsured: (sessionId: string) => void): { ensure(projectId: string, daemonInstanceId: string): Promise<void>; errorByProject: ReadonlyMap<string, string> };
// testIds: sessionsDrawerProjectHome(projectId), sessionsDrawerProjectHomeHint(projectId), sessionsDrawerProjectHomeError(projectId)
```
Plus the failing tests below. Bodies `// TODO(home-session): implement`.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** no — the drawer spec renders inside node 2's groups, the ensure tests read node 1's `home_agent`, and the sidebar assertion needs node 3's recorded `managed_codebase`.
**Concurrent with:** none
**Blocks:** `feature/project-home/home-session-tools` (5/7) and `feature/project-home/home-auto-stop` (6/7) — both act on a running project-home session.

Real dependency edges, as opposed to the branch line:

    n1 home-agent-setting → n4      n2 project-groups → n4      n3 activity-sidebar → n4      n4 home-session → n5, n6      n5 home-session-tools → n7

Waves: 1: n1, n2, n3 · 2: n4 · 3: n5, n6 · 4: n7 — line `1 1 1 2 3 3 4`.

## Affected Packages

- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — field + RPC
- **tddy-changeset**: [README.md](../../../packages/tddy-changeset/README.md) — `session_role`
- **tddy-session-activity**: [README.md](../../../packages/tddy-session-activity/README.md) — reader + enrichment
- **tddy-worktree-service**: [README.md](../../../packages/tddy-worktree-service/README.md) — home worktree
- **tddy-tool-engine**: [README.md](../../../packages/tddy-tool-engine/README.md) — read-only filter
- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — advertise the read-only catalog for the role
- **tddy-agent-launch**: [README.md](../../../packages/tddy-agent-launch/README.md) — ensure service
- **tddy-daemon-rpc**: [README.md](../../../packages/tddy-daemon-rpc/README.md) — RPC routing, `ExecuteTool` refusal
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — pinned entry

## Related Feature Documentation

- PRD: [PRD-2026-10-22-project-home-session.md](../../ft/daemon/1-WIP/PRD-2026-10-22-project-home-session.md)

## Summary

Adds the per-project home session: an ensure RPC that starts, resumes or returns a managed session
on a dedicated home worktree with a read-only tool catalog, and a pinned entry in each project group.

## Background

A standing, safe, one-click agent per project; the rest of the stack gives it tools.

## Scope

- [ ] `session_role` end to end
- [ ] Home worktree helper
- [ ] Ensure service + refusals
- [ ] Read-only filter (advertise + execute)
- [ ] Pinned drawer entry
- [ ] Tests

## Technical Changes

### State A
No session role exists. Managed sessions run the agent in a read-only context dir and route every code
op through `ExecuteTool` against a worktree; claude-cli is enforced via `build_claude_allowlist` +
`--disallowedTools`, cursor-cli is not enforceable. `ResumeSession` continues an agent's conversation.
Worktrees are per-session branches (`tddy-worktree-service`); nothing checks out a detached main ref.
The drawer has project groups (node 2) with no pinned row.

### State B
`EnsureProjectHomeSession` owns the home session's lifecycle by project; the home session's metadata
carries `session_role: project-home`; its worktree is `<worktrees root>/<project_id>/home`, detached at
`main_branch_ref`; `ExecuteTool` refuses non-read-only tools for that role; the drawer pins it.

### Delta
- **tddy-service**: field 35, RPC + messages.
- **tddy-changeset / tddy-session-activity**: `session_role` field, reader, enrichment.
- **tddy-worktree-service**: `home_worktree.rs` (new).
- **tddy-tool-engine**: `read_only.rs` (new); call in the dispatch path.
- **tddy-tools**: advertise `read_only_catalog()` when the session's role is `project-home` (role reaches the agent env at spawn).
- **tddy-agent-launch**: `svc_ensure_project_home.rs` (new): find by project + role, decide, build the start request (managed, home agent, home worktree as the selected placement), or fast-forward + resume.
- **tddy-daemon-rpc**: handler module for the RPC; `ExecuteTool` role check.
- **tddy-web**: `ProjectHomeEntry.tsx`, `useEnsureProjectHome.ts` (new); `SessionDrawer.tsx` passes the pinned entry into each project group.

## Testing Plan

RPC acceptance over the in-process daemon with the stub claude harness (`tddy-testing-commons`
stub scripts) and a real git fixture repo for the home worktree; a unit test table for
`is_read_only_tool`; a Cypress component spec for the pinned entry over `anInMemoryRpcBackend`.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `ensuring_a_project_home_starts_a_managed_session_on_a_home_worktree_at_the_main_ref` | `packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` (new — the stub-claude harness lives there) | started, role, managed, worktree HEAD = main ref |
| `ensuring_again_while_it_runs_returns_the_same_session` | same | running |
| `ensuring_a_stopped_home_session_fast_forwards_and_resumes_the_same_conversation` | same | resumed, worktree advanced, resume argv carries the conversation |
| `ensuring_without_a_home_agent_is_refused_and_starts_nothing` | same | failed_precondition |
| `ensuring_with_an_agent_that_cannot_enforce_managed_mode_is_refused` | same | failed_precondition naming harness |
| `a_project_home_session_is_listed_with_its_role` | same | ListSessions role |
| `a_project_home_session_cannot_write_or_run_a_shell_through_execute_tool` | `packages/tddy-daemon-rpc/tests/project_home_read_only_exec_tools_acceptance.rs` (new) | `PermissionDenied` for Write/StrReplace/Delete/Shell, worktree untouched |
| `a_project_home_session_still_reads_through_execute_tool` | same | Read allowed (passes already — guard) |
| `only_read_and_search_tools_are_read_only` / `the_read_only_catalog_offers_the_read_only_tools_in_catalog_order` | `packages/tddy-tool-engine/src/read_only.rs` (`#[cfg(test)]`) | filter table: Read, Grep, Glob, ReadLints, SemanticSearch |
| `each project group shows a pinned main entry first` | `packages/tddy-web/cypress/component/SessionsDrawerProjectHomeAcceptance.cy.tsx` (new) | pin + order |
| `clicking the pinned entry ensures the home session and selects it` | same | ensure → select |
| `the home session is not repeated in its group's rows` | same | de-dup |
| `a project without a home agent shows a hint instead of starting` | same | hint |
| `the Other group has no pinned entry` | same | Other |

## Decisions & Trade-offs
- **Dedicated home worktree** (developer's choice, 2026-10-22) over the project's main checkout: isolated from whatever branch the user has checked out; costs one checkout per project.
- **Read-only enforced in `ExecuteTool`**, not only by allowlist: an allowlist is spawn-time and harness-specific.
- **Refuse non-enforceable harnesses** rather than run them "guided": the session is advertised as read-only.
- **Complexity 5** — the boundary was weighed: it could split by capability into "daemon ensure + role (RPC-tested)" and "pinned drawer entry". Kept as one node by the developer's approval of the 7-node plan; revisit at the red step if the failing tests show the slice is too large.

## Red-phase notes

- Verified failing (2026-10-23): 6/6 `ensure_project_home_session_acceptance` (RPC unimplemented), `a_project_home_session_cannot_write_or_run_a_shell_through_execute_tool`, 2/2 `read_only` unit tests, 5/5 `SessionsDrawerProjectHomeAcceptance.cy.tsx` (no groups — node 2 — and no pinned entry).
- The home agent in the tests is a harness profile (`<data>/agents/home-claude.yaml`, `harness: claude-cli`); the cursor refusal uses a `harness: cursor-cli` profile. Green maps a `ListAgents` id to its harness the way `harness_profile_start` does.
- "Opens with the activity sidebar" is not a separate spec here: the started session is asserted `managed_codebase = true`, and node 3's `defaultActivitySidebarOpen` turns that into an open sidebar.
- `session_role` touched every `SessionMetadata` / reader / proto `SessionEntry` literal (50 sites); `cargo check --all-targets` clean on the 19 crates that build them; clippy `-D warnings` clean on tool-engine, worktree-service, agent-launch, session-lifecycle, daemon-rpc, changeset, session-activity.

## Green-phase notes

- Non-sandboxed claude-cli start had no way to run in a given checkout; `repo_path` is now honoured there (`adopt_checkout` in `claude_cli_spawn_steps.rs`, one extra parameter threaded through `spawn_claude_cli_session_inner`). Child/conversation spawns pass `""`. No branch added to `start_session_core`.
- `session_role` is recorded after the start returns (StartSessionRequest has no role field); ensures are serialized by a process-wide lock.
- Running = pid alive or the daemon still serves its terminal; otherwise stopped -> fast-forward + resume.
- `HarnessProfile.provider_account` is now `#[serde(default)]` (empty = the harness's own vendor): the red fixtures' profiles name none, and were silently skipped at load.
- ExecuteTool/StreamExecuteTool refuse non-read-only tools for the role (`role_guard.rs`). Advertising the read-only catalog via tddy-tools is NOT done (ListExecTools has no session id).
- Spec fix: `rowOrderIn` in the drawer spec filtered out a row's own status/label ids, which its prefix selector matched.
- Known gap: a project with no sessions has no group, so no pinned entry (grouping is node 2's rule).

## Technical Debt & Production Readiness

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-22-project-home-session-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: the developer asked for wave 2 to run without per-node review, 2026-10-23)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (Rust tests deferred to CI per coordinator; clippy -D warnings clean on 7 crates; drawer Cypress specs green)
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-project-home-session-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
