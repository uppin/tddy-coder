# Changeset: Project home agent setting

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto / contract change (field + RPC) across tddy-service, tddy-projects, tddy-daemon-rpc, tddy-web; storage + RPC acceptance + Cypress
**Stack**: #project-home 1/7. Base: `master`.
**PR**: [#696](https://github.com/uppin/tddy-coder/pull/696)

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-22-project-home-agent-setting-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [`2026-10-14-project-account-assignment-reaches-one-host.md`](../todo/2026-10-14-project-account-assignment-reaches-one-host.md) | ⚠ DURING | `SetProjectHomeAgent` routes to one host exactly like `SetProjectDefaultBranch`/`SetProjectAccounts`. Recorded, not fixed here — the fan-out is a semantics change the entry says needs its own decision. The PRD states the one-host behaviour. |
| [`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md) | ⚠ DURING | Unclaimed. The new coordinate handler goes in a new sibling module; `coordinate_handlers.rs` gains at most the dispatch line. |

## Responsibility

The per-project **home agent** setting, end to end: `ProjectEntry.home_agent`, the
`SetProjectHomeAgent` RPC (with catalog validation), its persistence in `projects.yaml`, its routing in
the daemon, and the **Home agent** picker on the Projects screen.

## Boundaries

- Does not start, resume or list any session — nothing reads `home_agent` here (node 4 does).
- Does not decide whether an agent can be enforced read-only; that refusal is node 4's at start time.
- Does not fan the write out to other hosts (todo 2026-10-14).
- Does not change any other project field or setter.

## Dependencies

None — root node, based on `master`.

## Draft PR contract

```proto
// project.proto
message ProjectEntry { /* … */ string home_agent = 10; }
rpc SetProjectHomeAgent(SetProjectHomeAgentRequest) returns (SetProjectHomeAgentResponse);
message SetProjectHomeAgentRequest { string session_token = 1; string project_id = 2; string home_agent = 3; string daemon_instance_id = 4; }
message SetProjectHomeAgentResponse { ProjectEntry project = 1; }
```
```rust
// tddy-projects
pub struct ProjectData { /* … */ #[serde(default, skip_serializing_if = "Option::is_none")] pub home_agent: Option<String> }
pub fn set_project_home_agent(projects_dir: &Path, project_id: &str, home_agent: Option<&str>) -> anyhow::Result<()>;
// ProjectHandler (tddy-projects/src/handler.rs)
async fn set_project_home_agent(&self, request: Request<SetProjectHomeAgentRequest>) -> Result<Response<SetProjectHomeAgentResponse>, Status>;
// tddy-daemon-kernel::peer_calls
pub async fn forward_set_project_home_agent(room: &CommonRoom, peer_instance_id: &str, request: &SetProjectHomeAgentRequest) -> Result<SetProjectHomeAgentResponse, Status>;
// tddy-daemon-rpc::project::home_agent_handler (new)
pub(crate) async fn set_project_home_agent_at_project_coordinate(&self, request) -> Result<Response<SetProjectHomeAgentResponse>, Status>;
```
```ts
// tddy-web
export function ProjectHomeAgent(props: { project: Pick<ProjectEntry, "projectId" | "homeAgent">; agents: Pick<AgentInfo, "id" | "label">[]; onChange(agentId: string): Promise<void> }): JSX.Element | null;
// ProjectsScreenProps gains (optional, so existing mounts keep compiling):
agents?: { id: string; label: string }[];
onSetHomeAgent?: (input: { projectId: string; homeAgent: string; daemonInstanceId: string }) => Promise<void>;
```

The setter follows the existing ones' `anyhow::Result<()>` shape; the catalog check happens in the
daemon-rpc handler before the write. Adding `ProjectData.home_agent` touched every `ProjectData` /
`ProjectEntry` struct literal in the workspace (`home_agent: None` / `String::new()`), and
`SetProjectHomeAgent` joins the closed `RESIDUAL_METHODS` list in
`packages/tddy-service/tests/unbundle_service_split.rs`.
Plus the failing acceptance tests below. Bodies `// TODO(home-agent-setting): implement`.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — its tests drive its own RPC over the in-memory daemon harness and mount its own picker against `anInMemoryRpcBackend`.
**Concurrent with:** `feature/project-home/project-groups` (2/7), `feature/project-home/activity-sidebar` (3/7)
**Blocks:** `feature/project-home/home-session` (4/7) — its ensure reads the stored `home_agent`.

Real dependency edges, as opposed to the branch line:

    n1 home-agent-setting → n4      n2 project-groups → n4      n3 activity-sidebar → n4      n4 home-session → n5, n6      n5 home-session-tools → n7

Waves: 1: n1, n2, n3 · 2: n4 · 3: n5, n6 · 4: n7 — line `1 1 1 2 3 3 4`.

## Affected Packages

- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — `project.proto` field + RPC
- **tddy-projects**: [README.md](../../../packages/tddy-projects/README.md) — `ProjectData.home_agent`, setter; `docs/project-service.md`
- **tddy-daemon-rpc**: [README.md](../../../packages/tddy-daemon-rpc/README.md) — coordinate handler
- **tddy-daemon-kernel**: [README.md](../../../packages/tddy-daemon-kernel/README.md) — `forward_set_project_home_agent` peer forward
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — Projects screen picker

## Related Feature Documentation

- PRD: [PRD-2026-10-22-project-home-agent-setting.md](../../ft/web/1-WIP/PRD-2026-10-22-project-home-agent-setting.md)

## Summary

Adds an optional per-project home agent, stored in `projects.yaml`, set through a new
`SetProjectHomeAgent` RPC validated against `ListAgents`, and edited from the Projects screen.

## Background

The project-home session (node 4) starts on click with no dialog, so its agent is a per-project
setting chosen ahead of time.

## Scope

- [ ] Proto field + RPC
- [ ] Storage + setter with catalog validation
- [ ] Daemon routing (new module)
- [ ] Projects screen picker
- [ ] Tests

## Technical Changes

### State A
`ProjectData` (`tddy-projects/src/project_storage.rs:22`) has no agent field. Per-field setters
(`set_project_default_branch` :252, `set_project_accounts` :300) are exposed through
`handler.rs`/`service.rs`/`lib.rs` and routed by `tddy-daemon-rpc/src/project/coordinate_handlers.rs`
to `req.daemon_instance_id`. The Projects screen (`components/projects/ProjectsScreen.tsx`) edits
default branch and accounts per row and sends to `group.hosts[0]`. Agent ids come from
`CatalogService.ListAgents` → `AgentInfo{id,label}`.

### State B
`ProjectData.home_agent: Option<String>`; `set_project_home_agent` writes it (None clears) after the
service checks the id against the catalog; `ListProjects` reports `home_agent`; the Projects screen
renders a `ProjectHomeAgent` picker per row.

### Delta
- **tddy-service**: `project.proto` — field 10, RPC + messages.
- **tddy-projects**: field, `set_project_home_agent`, service/handler trait method.
- **tddy-daemon-rpc**: `project/home_agent_handler.rs` (new), dispatch line in the coordinate handler, catalog lookup through the existing catalog port.
- **tddy-web**: `components/projects/ProjectHomeAgent.tsx` (new), wired into `ProjectsScreen.tsx`.

## Testing Plan

RPC acceptance over the in-process daemon (the `set_project_default_branch_acceptance.rs` pattern),
a storage unit test for the optional key, and a Cypress component spec with `mountWithRpc` +
`anInMemoryRpcBackend`.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `set_project_home_agent_persists_a_listed_agent_and_list_projects_reports_it` | `packages/tddy-daemon-rpc/tests/set_project_home_agent_acceptance.rs` (new) | write + read back |
| `set_project_home_agent_with_an_empty_id_clears_the_setting` | same | clear |
| `set_project_home_agent_refuses_an_agent_the_catalog_does_not_list` | same | `invalid_argument`, stored value unchanged |
| `a_project_row_written_without_home_agent_reads_as_unset` | `packages/tddy-projects/tests/project_home_agent_storage.rs` (new) | legacy rows (passes already — serde default; kept as the regression guard) |
| `setting_a_home_agent_stores_it_on_the_row` | same | write |
| `clearing_the_home_agent_writes_no_home_agent_key` | same | clear, no key written |
| `setting_a_home_agent_on_an_unknown_project_is_an_error` | same | unknown project |
| `the projects screen shows and changes a project's home agent` | `packages/tddy-web/cypress/component/ProjectsScreenHomeAgentAcceptance.cy.tsx` (new) | picker round-trip |
| `a refused home agent change shows the reason on the row` | same | inline error |

## Decisions & Trade-offs
- **Validated against `ListAgents`** rather than free text: a typo would otherwise surface only when the home session fails to start, far from where it was entered.
- **One-host write**, matching the existing setters (todo 2026-10-14).

## Red-phase notes

- Verified failing (2026-10-23): 3/3 `set_project_home_agent_acceptance` (unimplemented RPC), 2/4 `project_home_agent_storage` (setter unimplemented; legacy-read and unknown-project pass already and stay as guards), 2/2 `ProjectsScreenHomeAgentAcceptance.cy.tsx` (picker not rendered).
- Builds: `cargo build -p tddy-projects -p tddy-daemon-rpc -p tddy-service -p tddy-agent-launch --all-targets`; `cargo check --all-targets` on tddy-daemon, tddy-session-lifecycle, tddy-telegram-control, tddy-worktree-service, tddy-session-files, tddy-session-activity (every crate with a `ProjectData` literal); clippy `-D warnings` on tddy-projects, tddy-daemon-rpc, tddy-daemon-kernel; `unbundle_service_split` green.
- No separate pre-surface baseline was captured (the first baseline build raced the surface edits); master was green at 75878eb82.
- The catalog check needs the model registry (`list_assistants`), which `ProjectRpcHandler` does not hold today — green adds it from `DaemonSessionHost` or extracts the `list_agents` row assembly into a shared helper.

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

- [x] Record initial discovery (`2026-10-22-project-home-agent-setting-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: the developer asked for wave 2 to run without per-node review, 2026-10-23)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-project-home-agent-setting-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
