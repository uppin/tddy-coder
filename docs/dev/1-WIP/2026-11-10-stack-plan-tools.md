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
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — ~~role derivation for stack-node children~~ not touched: red moved the classification to tool-engine + daemon-rpc (see Decisions)

## Related Feature Documentation

- PRD: [PRD-2026-11-10-stack-plan-tools.md](../../ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md)
- [PR stacking § PR-management tools](../../ft/coder/pr-stacking.md#pr-management-tools), [Project concept § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools)

## Summary

`pr_stack_list`, `pr_stack_create`, `pr_stack_status`, `pr_read`, `pr_search`, `pr_comments`,
`pr_add_planned`, `pr_update_planned`, `pr_delete_planned`, `pr_set_status`, `pr_adopt` become daemon
exec tools addressing a project stack by `stack_id` (`daemon_instance_id` forwards). Offered to
orchestrator-role sessions (the stack their changeset names being their default); stack-node children
get status and read for their own stack. No recipe decides it (developer, 2026-11-14). GitHub calls use the project account's token.

## Background

Today the tools run inside `tddy-tools` against `TDDY_SESSION_DIR`, which only a pr-stack session has.
The developer decided (2026-11-10) that stacks are addressed by id through daemon-served exec tools, and
which sessions are offered them ("split by need").

## Responsibility

- Tool-engine module `stack_tools.rs`: names, schemas, `is_stack_tool`, `is_stack_read_tool`,
  `execute_stack_tool(port: &dyn StackToolPort, caller: &StackCaller, name, args)`.
- `StackCaller { session_id, project_id, role: Orchestrator { own_stack_id } | StackNode { stack_id } }`
  and the offered set per caller (developer, 2026-11-14: no recipe).
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
| n1 `orchestrator-role` (#878) | `is_orchestrator_role` (`tddy-changeset`), orchestrator sessions | caller classification: an orchestrator role → `StackCallerRole::Orchestrator` | add a role, change admission or placement |
| n2 `project-stack-store` (#879) | `ProjectStackStore::{create,read,update,list}`, `StackRecord`, `Changeset.stack_id`, `read_session_stack` / `update_stack_atomic` over the store | every tool reads/writes through it; a pr-stack session's default is its `stack_id` | change the store, its lock, or the record shape |

**Sequencing (red, 2026-11-13).** Both parents are green and in this tree; the tests drive the real
`is_orchestrator_role` and the real `ProjectStackStore` (no double). No test reaches a
`TODO(session-lifecycle-tools)` (n5) or `TODO(conversation-rotation)` (n6) stub: the acceptance
worlds call only the stack tools, `ListExecTools` and the store. n3–n6 are line neighbours only.

## Draft PR contract

- `stack_tools.rs` names/schemas/predicates and `StackToolPort` trait; `StackCaller`;
  daemon port struct with `TODO(stack-plan-tools)` bodies; catalogue entries.
- Failing tests below.

**Published in red (commit 2)** — every unimplemented body is `// TODO(stack-plan-tools): implement`:

- `tddy-tool-engine/src/stack_tools.rs` (new, `pub mod stack_tools`):
  - real: the eleven name consts, `STACK_ID`, `STACK_TOOL_NAMES`, `is_stack_tool`,
    `is_stack_read_tool`, `stack_tool_defs()` (each takes `daemon_instance_id`; all but
    `pr_stack_list`/`pr_stack_create` take `stack_id`); the types `StackCallerRole
    { Orchestrator { own_stack_id: Option<String> }, StackNode { stack_id } }`,
    `StackCaller { session_id, project_id, role }`, `StackCallerFacts { session_role,
    own_stack_id, parent_stack: Option<ParentStack { stack_id, binds_this_session }> }`,
    `PrAddress { Node, Number }`, `StackScope { project_id, stack_id: Option }`, `StackOp` (one
    variant per tool but list/create), and `trait StackToolPort { daemon_instance_id, list_stacks,
    create_stack(caller, title), run_stack_op(scope, op) -> Value }`.
  - stubbed: `stack_caller_role(facts)` (None), `is_stack_tool_offered_to` (false),
    `stack_tools_offered_to(Option<&StackCallerRole>)` (empty), `addressed_stack_id(caller, tool,
    named)` (Err), `execute_stack_tool(port, caller, tool, args)` (is_error).
- `tddy-daemon-rpc/src/exec_tool/stack_tools.rs` (new, `pub mod`):
  - real: `trait StackGithub: GithubPrApi + GithubPrInsightApi` (blanket impl), `trait
    StackGithubClients { client_for(repo, token) }`, `RealStackGithubClients`,
    `ExecToolRpcHandler::stack_tool_port(req, os_user, sessions_base) -> DaemonStackTools`.
  - stubbed: `stack_caller_of(sessions_base, session_id)` (Ok(None)),
    `ExecToolRpcHandler::run_stack_tool` (Unimplemented — **not yet routed** from `run_locally`),
    `impl StackToolPort for DaemonStackTools` (every op Err).
- `ProjectHomeHost.stack_github` late-bound cell + `ExecToolRpcHandler::stack_github_cell()`, and
  `RpcHandlers::attach_stack_github(Arc<dyn StackGithubClients>)` (real) — the seam the acceptance
  suite's recording GitHub goes through; unset means the REST API.
- Test support: `support::a_host_as_current_user_with_gh_at(id, peers, gh)` (daemon-rpc tests).
- `tddy-changeset` (developer, 2026-11-14): `StackRecord.title: Option<String>` (real, serde-default)
  and `ProjectStackStore::create_titled(project, created_by, title, stack)` (stubbed: ignores the
  title). `pr_stack_create` takes an optional `title`; the `pr_stack_list` entry carries `title`.
- Caller kinds per the developer's 2026-11-14 decisions (see Decisions): no recipe, no
  `PrStackOrchestrator`; own stack = `Changeset.stack_id`, a default for orchestrators only.

**Closed-world lists green must extend** (each pins an exact set today):

- `tddy-tool-engine/src/read_only.rs` — `is_project_home_allowed_tool` must admit the stack tools, or
  `role_guard::reject_refused_local_exec_tool` refuses main's call as read-only before
  `run_locally`; and its unit test `the_project_home_allowed_tools_are_…` (the `--allowedTools` list)
  gains the eleven if they join `project_home_offered_catalog`.
- `tddy-tools/src/exec_tool_admission.rs` — `EXEC_CATALOG_NAMES` and the dynamic router
  (`server.rs:343`, `project_home_remote_tool_defs`) gain `stack_tool_defs()`; its unit test
  `a_project_home_session_is_advertised_…` pins the project-home set.
- `tddy-daemon-rpc/tests/project_home_session_tools_acceptance.rs`
  `a_project_home_session_is_offered_its_read_only_tools_and_the_project_home_tools` pins main's
  `ListExecTools` answer exactly.
- `tddy-workflow-recipes` `PR_STACK_TOOL_NAMES` keeps the `mcp__tddy-tools__pr_*` names; the two
  existing reachability tests in `tddy-tools/tests/pr_stack_tool_dispatch_acceptance.rs` stay green
  only if `advertised_tool_defs()` includes `stack_tool_defs()` and `call_tool_by_name` forwards the
  eleven to `dispatch_dynamic_tool`.
- Packages green adds: **`tddy-pr-stack` as a `tddy-daemon-rpc` dependency** (already in its tree
  through `tddy-agent-launch`; check `rpc_handlers_shape.rs`). `stack_ops` is keyed by session dir
  today — green needs store-keyed variants (or the port does the read-modify-write through
  `ProjectStackStore::update`).

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — needs n2's store behaviour (every tool reads it) and n1's role (the
orchestrator caller case)
**Concurrent with:** n8, n9
**Blocks:** n10 `stack-git-tools` (same family, port and offering)

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [x] Tool family + caller classification + offered sets
- [x] Daemon port over store / stack_ops / pr_insight
- [x] `pr_stack_create`, `pr_stack_list`
- [x] Project-account GitHub credential
- [x] Forwarding to the owning host
- [x] Remove the eleven `tddy-tools` methods
- [ ] Docs via wrap (coder overview row)

## Technical Changes

### State A (Current)

Re-verified 2026-11-13 on `1c7c2e2a7` (n1 + n2 green underneath):

- `#[tool]` methods on `PermissionServer` (`tddy-tools/src/server.rs:944-1107`, file now 4,156
  lines), dispatched by name (`:515-529`); session from `orchestrator_dir()` (`:1185`,
  `TDDY_SESSION_DIR`), repo from `TDDY_REPO_DIR` (`:1192`, `:1221`), token over `TDDY_SOCKET`
  (`real_gh` `:1215`) — unchanged.
- **Moved (n2 green):** the tools no longer read `read_changeset().stack`. They go through
  `tddy_core::changeset::read_session_stack` (`server.rs:1293,1300,1537`) and `update_stack_atomic`
  (`:1257,1275,1623`), which resolve the session's `Changeset.stack_id` against
  `ProjectStackStore` at `<base>/projects/<project_id>/stacks/<stack_id>.yaml`
  (`tddy-changeset/src/changeset/stack.rs:300,338`; `project_stack_store.rs`). The session's project
  comes from `.session.yaml` (`session_stack_home`, `stack.rs:261`). `tddy_pr_stack::stack_ops`
  (`nodes.rs:41,126,196`, `adopt.rs`) is still **keyed by session dir** through the same seams.
- Exec-tool family pattern: `project_home_git_tools.rs` (consts 31-37, `is_*` :40, `*_tool_defs`
  :54, `execute_*` :153 over a port), catalogue `project_home_catalogue.rs`
  (`session_offered_catalog_for` :136, `project_home_catalog_with` :154), daemon `run_locally`
  (`exec_tool/project_home.rs:98`), forwarding `project_home_remote.rs` (`served_on_target_host`
  :167, `serve_project_home_for_caller_held_elsewhere` :53, owner check via peers' `ListSessions`).
  Sibling families from n4/n5: `project_home_turn_tools.rs`, `project_home_lifecycle_tools.rs`.
- `role_guard.rs:21` refuses every tool outside `is_project_home_allowed_tool` for an
  orchestrator-role session before `run_locally` is reached.
- `tddy-tools` advertises exec tools via `exec_tool_admission.rs` (`project_home_remote_tool_defs`
  :59, `is_advertised` :85) + `ListExecTools`; dynamic registration at `server.rs:343`.
- `session_tool_role` (`tddy-agent-launch/src/conversation_spawn.rs:21`) maps a role to
  `SessionToolRole {ProjectHome, ConversationSpawner, Ordinary}` — red kept stack classification
  out of it (see Decisions).
- GitHub: `tddy-github` `GithubPrApi` / `GithubPrInsightApi` (`pr_api.rs:78,222`),
  `RealGithubPrApi::with_token`; the daemon's project-account token is
  `tddy_agent_launch::session_acting_identity::{project_github_token, local_gh_github_token}`
  (as `pr_stack/ports.rs` — now 818 lines — uses for `RepointPlannedPr`).

### State B (Target)

- The eleven tools are daemon exec tools; `tddy-tools` has none of them as methods.

### Delta

- tool-engine new module + catalogue; daemon-rpc port + routing; tddy-tools removals and registration;
  agent-launch `session_tool_role` learns the stack-node and pr-stack cases.

## Implementation Milestones

- [x] Family + caller + offering
- [x] Read tools (status, read, search, comments, list)
- [x] Plan writers (add, update, delete, set status, adopt, create)
- [x] Forwarding
- [x] tddy-tools removal

## Testing Plan

**Level**: exec-tool acceptance in `tddy-daemon-rpc` against a project-home world with a fake GitHub
(`GithubPrApi` / `GithubPrInsightApi` fakes), stacks seeded through n2's store; forwarding over the
in-memory peer forwarder.

## Acceptance Tests

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/stack_plan_tools_acceptance.rs`
- `main_creates_a_stack_adds_planned_prs_and_reads_its_status_from_a_read_only_session`
- `a_session_acts_on_its_own_stack_without_naming_it` (was `a_pr_stack_orchestrators_tools_act_on_its_own_stack_without_naming_it`; no recipe, 2026-11-14)
- `a_stack_owned_by_another_host_is_read_and_edited_through_forwarding`
- `a_stack_node_session_is_offered_only_status_and_read_for_its_own_stack`
- `an_ordinary_session_is_offered_no_stack_tool_and_is_refused_one`
- `pr_update_planned_with_sync_pr_publishes_with_the_project_accounts_token`

### tddy-tool-engine — `packages/tddy-tool-engine/tests/stack_tools_catalogue.rs`
- `each_caller_kind_is_offered_its_stack_tool_set`

### tddy-tools — `packages/tddy-tools/tests/pr_stack_tool_dispatch_acceptance.rs` (rewritten)
- `the_plan_and_read_stack_tools_are_dispatched_to_the_daemon`

### Unit / integration tests (red, 6f)

- `tddy-tool-engine/src/stack_tools.rs` `mod tests` — classification (`stack_caller_role`, 7: no
  recipe), offering (`is_stack_tool_offered_to`, 2), addressing (`addressed_stack_id`, 8), the
  `pr_stack_create` schema's `title` (1), and `execute_stack_tool` over a recording port (9: create
  untitled and titled, list shape with `title`, add-planned op, search default state, read-by-node,
  both-addresses refusal, invalid arguments, port refusal prefix).
- `tddy-daemon-rpc/src/exec_tool/stack_tools.rs` `mod tests` — `stack_caller_of` over the real
  sessions layout and store (`stack_fixtures`): orchestrator, orchestrator owning its changeset's
  stack, a role-less pr-stack session classified none, node, missing record refused.
- `tddy-changeset/tests/project_stack_store_acceptance.rs` — a titled stack round-trips through
  create/update/read; a record written without a title still reads.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Split by need** (developer, 2026-11-10).
- **Project-account token**: a read-only session has no `TDDY_SOCKET` credential; the project's account
  is the identity every host already uses for the project.
- ~~**A pr-stack session keeps a default stack** so its recipe prompts need no `stack_id`.~~ Superseded 2026-11-14: an orchestrator's own stack (`Changeset.stack_id`) is its default; no recipe.
- Score 5 is a warning: if pass 2 shows the tool-engine family and the `tddy-tools` removal are
  separable by tool group, say so before green.
- **Red (2026-11-13, confirmed by the developer 2026-11-14):** stack classification lives in `tddy-tool-engine`
  (`stack_caller_role(StackCallerFacts)`, pure) with the daemon reading the facts
  (`stack_caller_of`), rather than new `SessionToolRole` variants in `tddy-agent-launch`
  `session_tool_role`: those variants would leak into the project-home caller gate
  (`exec_tool/project_home/caller.rs`). `tddy-agent-launch` is then untouched by this node.
- **Red:** role refusals (no stack role, or a tool the role is not offered) are `PermissionDenied`
  statuses, as the project-home gate's; addressing refusals (a node naming another stack, no
  `stack_id`) are `is_error` answers.
- **Developer, 2026-11-14 — caller kinds are decoupled from the recipe.** `PrStackOrchestrator` and
  every `Changeset.recipe` lookup are gone; the pr-stack recipe is deprecated and never decides a
  role. A caller is `Orchestrator { own_stack_id }` (holds an orchestrator role), `StackNode
  { stack_id }` (a node of its orchestrator's stack binds it), or none. **Own stack = the stack the
  session's changeset names (`Changeset.stack_id`)**, chosen over `created_by_session_id`: it is
  the one reference n2 already keeps per session, needs no store scan, and is unambiguous when a
  session created several stacks (main will). **Narrowest owner rule:** the own stack only gives
  an orchestrator a default — a session with no orchestrator role is offered no stack tool even
  when its changeset names a stack (so a role-less pr-stack session loses the tools). An orchestrator
  may still name any stack of its project; one owning none must name one (refused naming
  `stack_id` and `pr_stack_list`). `pr_stack_create` never makes the new stack the caller's own.
- **Developer, 2026-11-14 — stacks have a title, added in n7.** n2 is done and frozen, so
  `StackRecord.title: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`,
  so a record without one still reads) and `ProjectStackStore::create_titled` land here.
  `pr_stack_create { title? }` stores it; `pr_stack_list` shows it.
- **Developer, 2026-11-14 — cross-host gap accepted.** A forwarded call confirms only an
  orchestrator-role caller (the owner's `ListSessions` row carries no node binding), so a node session
  on another host cannot use its read tools there. Noted in
  `docs/dev/todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md`.
- **Developer, 2026-11-14 — classification stays in tool-engine + daemon**
  (`stack_caller_role` / `stack_caller_of`); `tddy-agent-launch` is untouched.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [x] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
