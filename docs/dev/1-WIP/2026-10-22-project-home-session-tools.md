# Changeset: Project-home session tools

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate (tddy-tool-engine, tddy-worktree-service, tddy-daemon-rpc, tddy-tools); daemon RPC seam (`ExecuteTool` → session listing / start); integration tests
**Stack**: #project-home 5/7. Base: `feature/project-home/home-session`.
**PR**: [#700](https://github.com/uppin/tddy-coder/pull/700)

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-22-project-home-session-tools-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [`packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md) | ⚠ DURING | New tools in `project_home_tools.rs`; `lib.rs` gains dispatch arms only. |
| [`packages/tddy-tools/docs/code-issues/oversized-file-server.md`](../../../packages/tddy-tools/docs/code-issues/oversized-file-server.md) | ⚠ DURING | No new static `#[tool]`s in `server.rs` — the tools arrive through the dynamic exec catalog. |
| [`packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-stream-execute-tool.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-stream-execute-tool.md) | ⚠ DURING | Route the new tools through a port method; no new branches in `stream_execute_tool`. |

## Responsibility

- `ListProjectSessions`, `StartProjectSession`, `ReadSessionWorktree`, `DiffSessionWorktree`: tool
  defs, schemas, role gate, execution.
- `tddy_worktree_service::session_target` — **the** resolver from a target session id to its worktree,
  branch, project and activity, with the "same project, not the home itself" rule. Node 7 consumes it.
- The `ExecuteTool` wiring that gives these tools the session listing and the start path.

## Boundaries

- No git write operations of any kind (node 7).
- No change to the read-only filter's existing contents (node 4) beyond adding these four names.
- No idle-stop behaviour (node 6).
- No web UI.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/project-home/home-session` (4/7) | `session_role`, `PROJECT_HOME_ROLE`, `EnsureProjectHomeSession`, `read_only.rs` | role gate reads the caller's role; tests start a home session via ensure; the four names join the read-only catalog | change ensure, the role field, or what node 4 lists as read-only |
| `feature/project-home/activity-sidebar` (3/7) | `managed_codebase` listed | `ListProjectSessions` reports it | change how it is recorded |

## Draft PR contract

```rust
// tddy-worktree-service::session_target (new)
pub struct SessionTarget { pub session_id: String, pub project_id: String, pub worktree: PathBuf, pub branch: String, pub base_ref: String, pub is_active: bool }
pub struct TargetRecord { pub session_id, pub project_id, pub worktree: Option<PathBuf>, pub branch, pub base_ref, pub is_active }
pub struct HomeCaller { pub session_id: String, pub project_id: String }
pub enum SessionTargetError { NotFound(String), OtherProject(String), IsHome, NoWorktree(String) }
pub fn resolve_session_target(records: &[TargetRecord], home: &HomeCaller, target_session_id: &str) -> Result<SessionTarget, SessionTargetError>;  // pure; the daemon builds `records` from `.session.yaml`
// tddy-tool-engine::project_home_tools (new)
pub const LIST_PROJECT_SESSIONS: &str = "ListProjectSessions";
pub const START_PROJECT_SESSION: &str = "StartProjectSession";
pub const READ_SESSION_WORKTREE: &str = "ReadSessionWorktree";
pub const DIFF_SESSION_WORKTREE: &str = "DiffSessionWorktree";
pub fn project_home_tool_defs() -> Vec<ToolDef>;
pub struct ListedSession { session_id, branch, worktree, is_active, agent_status, pending_elicitation }
pub struct StartProjectSessionArgs { session_type, prompt, agent, new_branch_name }
pub trait ProjectHomePort: Send + Sync {
    fn list_sessions(&self, project_id: &str, home_session_id: &str) -> anyhow::Result<Vec<ListedSession>>;
    fn start_session(&self, project_id: &str, home_session_id: &str, args: StartProjectSessionArgs) -> anyhow::Result<String>;
}
// Wire contract the tests pin: a role refusal is `PermissionDenied`; a target/path refusal is an
// `is_error` result whose message names it ("another project", "outside the worktree").
// ListProjectSessions → {"sessions":[{session_id, worktree, is_active, agent_status, …}]};
// StartProjectSession → {"session_id"}; ReadSessionWorktree {session_id, path}; DiffSessionWorktree {session_id}
```
Plus the failing tests below. Bodies `// TODO(home-session-tools): implement`.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — the acceptance tests call the tools from a real project-home session, which needs node 4's ensure to work.
**Concurrent with:** `feature/project-home/home-auto-stop` (6/7)
**Blocks:** `feature/project-home/home-git-tools` (7/7) — it resolves targets with `resolve_session_target`.

Real dependency edges, as opposed to the branch line:

    n1 home-agent-setting → n4      n2 project-groups → n4      n3 activity-sidebar → n4      n4 home-session → n5, n6      n5 home-session-tools → n7

Waves: 1: n1, n2, n3 · 2: n4 · 3: n5, n6 · 4: n7 — line `1 1 1 2 3 3 4`.

## Affected Packages

- **tddy-tool-engine**: [README.md](../../../packages/tddy-tool-engine/README.md) — tools
- **tddy-worktree-service**: [README.md](../../../packages/tddy-worktree-service/README.md) — target resolver
- **tddy-daemon-rpc**: [README.md](../../../packages/tddy-daemon-rpc/README.md) — `ExecuteTool` port wiring
- **tddy-tools**: [README.md](../../../packages/tddy-tools/README.md) — advertisement via the dynamic catalog (no new static tools)

## Related Feature Documentation

- PRD: [PRD-2026-10-22-project-home-session-tools.md](../../ft/daemon/1-WIP/PRD-2026-10-22-project-home-session-tools.md)

## Summary

Four role-gated exec tools that let the project-home agent list, start and read its project's sessions.

## Background

Read-only by design; these tools are its reach. No tool lists sessions or reads another session's
worktree today (`subagent_*` cover only a session's own roster).

## Scope

- [ ] `session_target` resolver
- [ ] Four tools + role gate
- [ ] `ExecuteTool` port wiring
- [ ] Tests

## Technical Changes

### State A
Exec tools resolve the worktree from the **calling** session (`ExecuteTool` with `session_id`).
`spawn_conversation`/`pr_spawn_child` start sessions via the `TDDY_SOCKET` relay (not available to a
daemon-hosted managed session). `branch_owner.rs` has the `SessionListing` port and `SessionClaim`.

### State B
A project-home session's `ExecuteTool` can name another session as a target; the daemon resolves it
through `session_target` and serves list / start / read / diff.

### Delta
- **tddy-worktree-service**: `session_target.rs` (new), reusing `SessionListing`.
- **tddy-tool-engine**: `project_home_tools.rs` (new); dispatch arms; names added to the read-only catalog.
- **tddy-daemon-rpc**: implement `ProjectHomePort` over the session reader and the start service.
- **tddy-tools**: none beyond what the dynamic catalog already advertises — verify in tests.

## Testing Plan

RPC acceptance over the in-process daemon: ensure a home session (stub harness), start ordinary
sessions in the same and another project, then drive the four tools through `ExecuteTool` as the home
session; unit tests for the resolver's rules.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_project_home_session_lists_its_projects_sessions_with_status` | `packages/tddy-daemon-rpc/tests/project_home_session_tools_acceptance.rs` (new) | list |
| `a_project_home_session_starts_a_session_tagged_with_itself_as_orchestrator` | same | start |
| `a_project_home_session_reads_a_file_and_lists_a_directory_in_another_worktree` | same | read |
| `a_project_home_session_gets_a_target_worktrees_diff_against_its_base` | same | diff |
| `a_target_in_another_project_is_refused` | same | scope |
| `a_path_escaping_the_target_worktree_is_refused` | same | containment |
| `a_session_without_the_project_home_role_cannot_call_the_tools` | same | role gate |
| `resolving_a_session_of_the_same_project_returns_its_worktree_and_activity` | `packages/tddy-worktree-service/src/session_target.rs` (`#[cfg(test)]`) | happy path |
| `resolving_the_home_session_itself_is_refused` | same | IsHome |
| `resolving_a_session_of_another_project_is_refused` / `resolving_a_session_without_a_worktree_is_refused` | same | OtherProject / NoWorktree |
| `resolving_an_unknown_session_is_not_found` | same | NotFound |

## Decisions & Trade-offs
- **Exec tools over `ExecuteTool`**, not new static MCP tools in `tddy-tools`: a managed agent already reaches the daemon only through that path, and `server.rs` is over budget.
- **Role-gated**: these tools reach across sessions; only the read-only home agent gets them.

## Red-phase notes

- Verified failing (2026-10-23): 7/7 `project_home_session_tools_acceptance` (tools not in the catalog / no role gate), 4/5 `session_target` unit tests (`resolving_an_unknown_session_is_not_found` passes already — the stub's only answer — and stays as a guard).

## Green status

- Local, scoped builds (developer instruction 2026-10-23: build only, Rust tests run in CI): `./dev cargo check -p tddy-worktree-service -p tddy-tool-engine -p tddy-daemon-rpc --all-targets` exits 0 with no warnings. Clippy per crate: see the result line in the commit message.
- Tests: deferred to CI (`project_home_session_tools_acceptance` 7, `session_target` unit tests 5).
- Implemented:
  - `tddy_worktree_service::session_target::resolve_session_target`.
  - `tddy_tool_engine::project_home_tools` (new): the four tool defs, the executor, the target and path refusals. A refused path is an `is_error` result naming "outside the worktree"; a refused target names "another project" or the other `SessionTargetError` text. `project_home_offered_catalog()` is the set a project-home session is offered: the tools `is_project_home_allowed_tool` admits, plus the four.
  - `tddy_tool_engine::project_home_diff` (new): the diff over a temporary `GIT_INDEX_FILE` (`read-tree HEAD`, `add -A`, `diff --cached <base>`), so untracked files appear and the worktree's index is never written.
  - `tddy_daemon_rpc::exec_tool::project_home` (new): the role gate (`PermissionDenied` unless the caller's `.session.yaml` role is `project-home`), the `ProjectHomePort`, the orchestrator tag written to the new changeset, the call record, and the session-scoped `ListExecTools` answer.
  - `read_only::is_project_home_allowed_tool` adds the four names to the project-home allow-list. `is_read_only_tool` and `read_only_catalog` are unchanged. `role_guard` uses the new check. `ports.rs` gains only `?` at the two call sites, no new branch in `stream_execute_tool`.
- Decision (a), start of any session type: a late-bound launch. `DaemonSessionHost::with_rpc_families` hands the families a launch snapshot of the host through `DaemonRpcFamilies::attach_session_start` (a default no-op, so `SomeRpcFamilies` in the lifecycle acceptance test keeps compiling). The snapshot holds its families through a `Weak` forwarder (`WeakRpcFamilies`), so the families → snapshot → families cycle is never formed. `RpcHandlers` keeps the set-once cell (`Arc<OnceLock<LaunchSessions>>`) that `ExecToolRpcHandler` reads. A project-home `StartProjectSession` calls the host's own `start_session_at_session_coordinate` as the caller's token, so every session type the web's `StartSession` accepts starts from it, with families available. The workspace-only restriction and its FIXME are gone. A handler built with `RpcHandlers::from_host` but never installed refuses with `FAILED_PRECONDITION` naming the missing attach.
  - `ListProjectSessions` now reads the host's own `list_sessions_at_session_coordinate`: `pending_elicitation`, `branch` and the activity status come from the same enrichment the web sees. Target records (for the resolver) still come from the session store, since they need every project's worktrees.
- Decision (b), tool discovery: a session id on `ListExecTools`. `ListExecToolsRequest` gains `session_id = 3`; the daemon reads the role from that session's own record at query time (`ExecToolRpcHandler::session_tool_catalog`), and answers the project-home offered set or the unchanged catalog without the project-home tools. The alternative, the role passed at spawn, needs a `StartSessionRequest` role field and is recorded only after `start_session_core` returns (the backlog's window), so it is the larger and the less sound option. tddy-tools already carries the session id (`TDDY_REMOTE_SESSION_ID`).
  - `tddy_session_tool_client::session_exec_tool_answer` asks that question over `DaemonHttp` and `DaemonUds`, the two transports that carry a session identity, with a 10-second deadline. It answers `Admitted`, `CannotAsk(transport)`, or `Failed(reason)`.
  - `tddy_tools::exec_tool_admission` (new): the four project-home tools are registered at construction so they dispatch, and `list_tools` (async, asked per listing) advertises an exec-catalog tool only when the daemon admits it. A tool outside the exec catalog is untouched. `advertised_tools()` keeps its static answer.
  - **Refusal (developer decision, 2026-10-23):** a failed ask (`Failed`: the daemon answered with an error, or missed the deadline) is refused. `tools/list` returns an MCP error naming the reason, and the error is logged at error level. This withholds every tool the MCP server offers, the static workflow and subagent tools included, not only the exec tools. That is louder than withholding exec tools alone, and it is the choice made here; say if you want exec-only withholding instead.
  - **Open, stopped for decision:** `CannotAsk` (sandbox socket, LiveKit) keeps the static exec catalog, pending a decision. Refusing it removes every exec tool from the sessions listed under "Open" below. No project-home tool is admitted on those transports. A missing transport registers no exec tools at all, as before.
  - The generated TypeScript for `exec_tools.proto` is regenerated (`scripts/generated-code.sh write`): `packages/tddy-web/src/gen/exec_tools_pb.ts` and `packages/tddy-rust-typescript-tests/gen/exec_tools_pb.ts`.
  - Seven existing `ListExecToolsRequest { .. }` literals in five test files gained `..Default::default()` for the new field. No assertion changed.
- Deviations from the Draft PR contract:
  - `ProjectHomePort` methods are `async` (`async_trait`), since the daemon reads the store and starts a session on disk.
  - A third port method, `target_records`, was added. The resolver needs every session's project and worktree, which `list_sessions` (the caller's project only) does not give.
- Open, stopped for decision (🔴):
  - **Transports that cannot ask.** The sandbox socket (`TDDY_SANDBOX_TOOL_IPC`) relays only `ExecuteTool`, the roster RPCs and `ConversationWorktree` (`packages/tddy-sandbox-runner/src/runner.rs`, `tddy_tool_engine::IN_JAIL_RELAYABLE_EXEC_TOOLS`), so `ListExecTools` is `not_found` there. LiveKit has no `ListExecTools` path. Refusing `CannotAsk` would remove every exec tool (Read, Write, Shell, Grep, Glob, ReadLints, SemanticSearch, Await, StrReplace, Delete, and the LSP tools) from: (a) every sandboxed (jailed) agent session, which sets the sandbox socket, and (b) every split session whose agent runs here and whose worktree is on another host (LiveKit). An incomplete LiveKit configuration already refuses its own calls. Options: relay `ListExecTools` over the sandbox socket the way `ConversationWorktree` is relayed, rebinding the session id in `bind_to_this_session`, and add the LiveKit path the same way; keep the static catalog for these transports; or refuse them as decided and accept the loss.
  - **Role-aware allowlist premise.** The claude `--allowedTools` list is built in only three places: the sandboxed runner's argv (`append_claude_mcp_args`), split-session agents (`split_claude_extra_args`), and the print-mode backend (`tddy-agent-backend`). The home session as `EnsureProjectHomeSession` starts it is `claude-cli` with `managed_codebase: true` and no `sandbox`, so it takes the co-located spawn, which passes no `--allowedTools`. A role-aware allowlist therefore does not reach the home session as started today. Making it reach would need `session_role` on `StartSessionRequest` written to the initial metadata before spawn (extending n4's ensure), and a role-aware `--allowedTools` in the co-located claude spawn (`build_claude_base_argv` callers). That is a new spawn-argv path for co-located claude-cli sessions, and the developer's call. Not done.
  - `ListProjectSessions` and the discovery answer are not available over `SandboxIpc` or LiveKit. A project-home session on those transports is offered the static catalog until the decision above is taken.
  - Backlog `docs/dev/todo/2026-10-23-project-home-read-only-catalog-not-advertised.md` is **narrowed, not closed**. Its advertisement half is resolved on `DaemonHttp`/`DaemonUds`. The allowlist (above) and the role-recorded-after-start window, where the ensure records the role after `start_session_core` returns, remain, so it does not fully resolve.
- The acceptance world uses `workspace` sessions and marks one `project-home` in its metadata, so these tests need node 4's role field and `ExecuteTool` role path but not its ensure behaviour; node 4's read-only filter must let these four names through for the role.
- `StartProjectSession` from a daemon-hosted session goes through the daemon's start path (not the `TDDY_SOCKET` relay `spawn_conversation` uses), with `orchestrator_session_id` = the home session.

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

- [x] Record initial discovery (`2026-10-22-project-home-session-tools-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: the developer asked for wave 2 to run without per-node review, 2026-10-23)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (start deferred to workspace sessions; see Green status)
- [x] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests for touched packages; full-workspace result from CI (tests deferred to CI)
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-project-home-session-tools-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
