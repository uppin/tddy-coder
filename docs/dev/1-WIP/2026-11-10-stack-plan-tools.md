# Changeset: stack tools served by the daemon, addressed by stack id

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Feature
**Complexity**: 5 — more than one crate (tool-engine, daemon-rpc, tddy-tools, pr-stack), cross-host forwarding, a credential source change (session → project account), eleven tools moved across a process boundary
**Stack**: `#orchestrators` 7/11 · branch `feature/orchestrators/stack-plan-tools` · base `feature/orchestrators/conversation-rotation` (#883)

## Initial Discovery

[2026-11-10-stack-plan-tools-initial-discovery.md](./2026-11-10-stack-plan-tools-initial-discovery.md)

## Prerequisites

### ⚠ DURING — `server.rs` is 2,677 production lines — [`oversized-file-server.md`](../../../packages/tddy-tools/docs/code-issues/oversized-file-server.md), [`2026-10-08-tddy-tools-server-rs-…`](../todo/2026-10-08-tddy-tools-server-rs-is-3-9k-lines-and-the-crate-mixes-several-responsibilities.md)

This node **removes** eleven `#[tool]` methods and their helpers from `server.rs`, so the file shrinks;
the record is re-measured at wrap and narrowed, not closed (n10 removes six more). Not claimed.

### ⚠ DURING — PR-Stack full-control follow-ups — [`2026-07-30-pr-stack-full-control-follow-ups.md`](../todo/2026-07-30-pr-stack-full-control-follow-ups.md)

Its "lift the tools out of `server.rs`" item is overtaken for these eleven tools: they leave the crate.
The `PrSearchState` enum item is done here if `pr_search`'s schema is rewritten anyway (decided in red).
Not claimed.

### ⚠ DURING — The agent sees StartSession's host and recipe as free strings — [`2026-10-25-start-session-mcp-schema-has-no-host-or-recipe-choices.md`](../todo/2026-10-25-start-session-mcp-schema-has-no-host-or-recipe-choices.md)

`stack_id` and `daemon_instance_id` are free strings for the same reason; `pr_stack_list` is how the
agent learns them. Not claimed.

### ⚠ DURING — `pr_stack/ports.rs` is 808 lines — [`oversized-file-pr-stack-ports.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md)

The tool port is a new module `exec_tool/stack_tools.rs`, not more of `pr_stack/ports.rs`.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10).

## Affected Packages

- [tddy-tool-engine](../../../packages/tddy-tool-engine/README.md) — `stack_tools` family, catalogue roles
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — stack-tools port, routing, forwarding
- [tddy-pr-stack](../../../packages/tddy-pr-stack/README.md) — `stack_ops` / `pr_insight` called from the daemon
- [tddy-tools](../../../packages/tddy-tools/README.md) — eleven tools removed; dynamic exec-tool registration
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — role derivation for stack-node children

## Related Feature Documentation

- PRD: [PRD-2026-11-10-stack-plan-tools.md](../../ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md)
- [PR stacking § PR-management tools](../../ft/coder/pr-stacking.md#pr-management-tools), [Project concept § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools)

## Summary

`pr_stack_list`, `pr_stack_create`, `pr_stack_status`, `pr_read`, `pr_search`, `pr_comments`,
`pr_add_planned`, `pr_update_planned`, `pr_delete_planned`, `pr_set_status`, `pr_adopt` become daemon
exec tools addressing a project stack by `stack_id` (`daemon_instance_id` forwards). Offered to
orchestrator-role sessions and pr-stack orchestrators (own stack by default); stack-node children get
status and read for their own stack. GitHub calls use the project account's token.

## Background

Today the tools run inside `tddy-tools` against `TDDY_SESSION_DIR`, which only a pr-stack session has.
The developer decided (2026-11-10) that stacks are addressed by id through daemon-served exec tools, and
which sessions are offered them ("split by need").

## Responsibility

- Tool-engine module `stack_tools.rs`: names, schemas, `is_stack_tool`, `is_stack_read_tool`,
  `execute_stack_tool(port: &dyn StackToolPort, caller: &StackCaller, name, args)`.
- `StackCaller { session_id, project_id, role: Orchestrator | PrStackOrchestrator { own_stack_id } |
  StackNode { stack_id } }` and the offered set per caller.
- Daemon `StackToolPort` over `ProjectStackStore` + `stack_ops` + `pr_insight`, project-account GitHub
  credential, `served_on_target_host` forwarding.
- `pr_stack_create { title? } -> { stack_id }`; `pr_stack_list -> { stacks: [...] }`.
- Removal of the eleven `tddy-tools` methods; `tddy-tools` advertises them from `ListExecTools`.

## Boundaries

- Does **not** move `pr_merge`, `pr_close`, `pr_repoint`, `pr_set_parents`, `pr_resolve_conflicts`,
  `pr_spawn_child` (n10).
- Does **not** change PrStackService RPCs or the web (n9).
- Does **not** change the store (n2) or role predicate (n1).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `orchestrator-role` (#878) | `is_orchestrator_role` (`tddy-changeset`), orchestrator sessions | caller classification: an orchestrator role → `StackCaller::Orchestrator` | add a role, change admission or placement |
| n2 `project-stack-store` (#879) | `ProjectStackStore::{create,read,update,list}`, `Changeset.stack_id` | every tool reads/writes through it; a pr-stack session's default is its `stack_id` | change the store, its lock, or the record shape |

## Draft PR contract

- `stack_tools.rs` names/schemas/predicates and `StackToolPort` trait; `StackCaller`;
  daemon port struct with `TODO(stack-plan-tools)` bodies; catalogue entries.
- Failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — needs n2's store behaviour (every tool reads it) and n1's role (the
orchestrator caller case)
**Concurrent with:** n8, n9
**Blocks:** n10 `stack-git-tools` (same family, port and offering)

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] Tool family + caller classification + offered sets
- [ ] Daemon port over store / stack_ops / pr_insight
- [ ] `pr_stack_create`, `pr_stack_list`
- [ ] Project-account GitHub credential
- [ ] Forwarding to the owning host
- [ ] Remove the eleven `tddy-tools` methods
- [ ] Docs via wrap (coder overview row)

## Technical Changes

### State A (Current)

- `#[tool]` methods on `PermissionServer` (`tddy-tools/src/server.rs:944-1107`), dispatched by name
  (`:515-529`); session from `orchestrator_dir()` (`:1185`, `TDDY_SESSION_DIR`), repo from
  `TDDY_REPO_DIR` (`:1192`, `:1221`), token over `TDDY_SOCKET` (`:1215`).
- Exec-tool family pattern: `project_home_git_tools.rs` (consts, `is_*`, `*_tool_defs`, `execute_*`
  over a port), catalogue `project_home_catalogue.rs:131,153`, daemon `run_locally`
  (`exec_tool/project_home.rs:93`), forwarding `project_home_remote.rs:174`.
- `tddy-tools` advertises exec tools via `exec_tool_admission.rs` + `ListExecTools`.

### State B (Target)

- The eleven tools are daemon exec tools; `tddy-tools` has none of them as methods.

### Delta

- tool-engine new module + catalogue; daemon-rpc port + routing; tddy-tools removals and registration;
  agent-launch `session_tool_role` learns the stack-node and pr-stack cases.

## Implementation Milestones

- [ ] Family + caller + offering
- [ ] Read tools (status, read, search, comments, list)
- [ ] Plan writers (add, update, delete, set status, adopt, create)
- [ ] Forwarding
- [ ] tddy-tools removal

## Testing Plan

**Level**: exec-tool acceptance in `tddy-daemon-rpc` against a project-home world with a fake GitHub
(`GithubPrApi` / `GithubPrInsightApi` fakes), stacks seeded through n2's store; forwarding over the
in-memory peer forwarder.

## Acceptance Tests

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/stack_plan_tools_acceptance.rs`
- `main_creates_a_stack_adds_planned_prs_and_reads_its_status_from_a_read_only_session`
- `a_pr_stack_orchestrators_tools_act_on_its_own_stack_without_naming_it`
- `a_stack_owned_by_another_host_is_read_and_edited_through_forwarding`
- `a_stack_node_session_is_offered_only_status_and_read_for_its_own_stack`
- `an_ordinary_session_is_offered_no_stack_tool_and_is_refused_one`
- `pr_update_planned_with_sync_pr_publishes_with_the_project_accounts_token`

### tddy-tool-engine — `packages/tddy-tool-engine/tests/stack_tools_catalogue.rs`
- `each_caller_kind_is_offered_its_stack_tool_set`

### tddy-tools — `packages/tddy-tools/tests/pr_stack_tool_dispatch_acceptance.rs` (rewritten)
- `the_plan_and_read_stack_tools_are_dispatched_to_the_daemon`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Split by need** (developer, 2026-11-10).
- **Project-account token**: a read-only session has no `TDDY_SOCKET` credential; the project's account
  is the identity every host already uses for the project.
- **A pr-stack session keeps a default stack** so its recipe prompts need no `stack_id`.
- Score 5 is a warning: if pass 2 shows the tool-engine family and the `tddy-tools` removal are
  separable by tool group, say so before green.

## Refactoring Needed

### From @red (TDD Red Phase)
### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### Change Validation (@validate-changes)
### Test Validation (@validate-tests)
### Production Readiness (@prod-ready)
### Code Quality (@analyze-clean-code)

## Successor PRs

n8 `feature/orchestrators/orchestrator-composer`.

## TODO

- [x] Record initial discovery (`2026-11-10-stack-plan-tools-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
