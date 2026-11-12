# Changeset: orchestrator sessions — any number of read-only sessions with main's reach

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Feature
**Complexity**: 4 — more than one crate (changeset, agent-launch, tool-engine, daemon-rpc, two web packages), a role/authorization contract change across hosts
**Stack**: `#orchestrators` 1/11 · branch `feature/orchestrators/orchestrator-role` · base `master`

## Initial Discovery

[2026-11-10-orchestrator-role-initial-discovery.md](./2026-11-10-orchestrator-role-initial-discovery.md)

## Prerequisites

### ✅ RESOLVED HERE — StartSession's orchestrator is written after the start returns — [`2026-10-25-start-session-orchestrator-recorded-after-start.md`](../todo/2026-10-25-start-session-orchestrator-recorded-after-start.md)

⛔ Blocking for this node: with several orchestrators per project, which one a child belongs to decides
who may answer its prompts. `orchestrator_session_id` / `orchestrator_daemon_instance_id` go into the
child's initial record, before its agent spawns; `record_orchestrator` (`TODO(start-parity)`) is removed.

### ⚠ DURING — A forwarded conversation spawner is refused the start tools — [`2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md`](../todo/2026-10-26-forwarded-conversation-spawner-is-refused-the-start-tools.md)

This node generalises the owner check in `project_home_remote.rs` from `== project-home` to *an
orchestrator role*, which is what lets a forwarded orchestrator be served. The spawner half (the
listing carries no recipe) stays open; not claimed.

### ⚠ DURING — A sandboxed main carries its role but not the main's tool wiring — [`2026-10-24-sandboxed-main-needs-session-role.md`](../todo/2026-10-24-sandboxed-main-needs-session-role.md)

Orchestrators do not inherit that gap: an orchestrator start naming `sandbox` is refused with
`INVALID_ARGUMENT` (see Decisions). Main keeps today's behaviour.

### ⚠ DURING — The agent sees StartSession's host and recipe as free strings — [`2026-10-25-start-session-mcp-schema-has-no-host-or-recipe-choices.md`](../todo/2026-10-25-start-session-mcp-schema-has-no-host-or-recipe-choices.md)

The new `session_role` argument is a one-value enum in the static `project_home_tool_defs()` schema, so
it does not depend on the `ListExecTools` choices that entry says are lost.

### ⚠ DURING — code issues on the start path

- [`complexity-svc-start-session-core-start-session-core.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md) — the role admission goes in `refuse_a_requested_role`, the placement in `project_home_start_spec.rs`; `start_here` / `start_session_on_request` must not grow.
- [`oversized-file-create-session-pane.md`](../../../packages/tddy-session-create-web/docs/code-issues/oversized-file-create-session-pane.md) — the toggle is its own field component; `CreateSessionPane.tsx` (581) must not grow.

### ⚠ DURING — open PR #876 (`feature/agent-tools/sweep`)

Edits `connection_service.rs` and `tddy-agent-launch` start code. **Developer decision (2026-11-10): pass 1
now, pass 2 waits for #876 to merge**, then this branch rebases onto it.

## Affected Packages

- [tddy-changeset](../../../packages/tddy-changeset/README.md) — `ORCHESTRATOR_ROLE`, `is_orchestrator_role`
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — role admission, placement, launch wiring, token mint, start-time orchestrator link
- [tddy-tool-engine](../../../packages/tddy-tool-engine/README.md) — `SessionToolRole`, `StartSession` argument
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — read-only guard, caller gate, owner check, write-target refusal
- [tddy-worktree-service](../../../packages/tddy-worktree-service/) — target resolution: orchestrator targets
- [tddy-session-create-web](../../../packages/tddy-session-create-web/README.md) — Session orchestrator toggle
- [tddy-session-web](../../../packages/tddy-session-web/docs/README.md) — drawer row marker

## Related Feature Documentation

- PRD: [PRD-2026-11-10-orchestrator-sessions.md](../../ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md)
- [Project concept § Project-home session](../../ft/daemon/project-concept.md#project-home-session), [§ Project-home tools](../../ft/daemon/project-concept.md#project-home-tools)
- [Project-home auto-stop](../../ft/daemon/project-home-auto-stop.md), [Session drawer](../../ft/web/session-drawer.md)

## Summary

Adds the `orchestrator` session role. It is requested on `StartSession` (by a person, or by an agent
that itself holds an orchestrator role), placed by the daemon exactly as main is (home worktree,
managed, read-only, an agent with `project_home_main`), and granted main's tool set and launch wiring.
Main stays the recorded, pinned, auto-stopped session.

## Background

One project-home session per host is the only read-only session with reach over the project. The
developer wants unlimited such sessions, sharing the home worktree, creatable from New session and by
main, with only main auto-stopped (interview 2026-11-10).

## Responsibility

- The role value `orchestrator` and the predicate `is_orchestrator_role(role)` (true for `project-home`
  and `orchestrator`) — **owned here**; every later node that asks "is this an orchestrator?" calls it.
- Requested-role admission, orchestrator placement, launch wiring, the cross-host owner check, the
  write-target refusal, the start-time orchestrator link.
- `StartSession` exec argument `session_role` (one value: `orchestrator`).
- Web: the New-session **Session orchestrator** toggle and the drawer row marker.

## Boundaries

- Does **not** add a composer, a "+" button or a gear (n8, n11).
- Does **not** add effort (n3) or any new session-control tool (n4, n5).
- Does **not** change `home_session_id`, `EnsureProjectHomeSession`, Replace main or auto-stop's filter.
- Does **not** touch stacks (n2, n7, n9, n10).

## Dependencies

Root node: none. Base `master` — #876 merged (`8797a715a`) before pass 2; this branch was rebased onto
`4cf668a4a`. Every State A fact was re-checked against that tip (2026-11-12): two line numbers moved
(noted inline), nothing changed shape.

## Draft PR contract

The first push of this PR (pass 2, commit 2) publishes:

- `tddy_changeset::session_metadata::{ORCHESTRATOR_ROLE, is_orchestrator_role(role: &str) -> bool}` —
  signatures final. The body is a stub that keeps today's answer (`role == PROJECT_HOME_ROLE`,
  `TODO(orchestrator-role)`), so its unit test is red until green; children call the signature.
- `tddy_tool_engine::project_home_catalogue::SessionToolRole` unchanged in shape. The mapping from a
  role to `ProjectHome` is `tddy_agent_launch::conversation_spawn::session_tool_role` (it lives in
  agent-launch, not tool-engine), marked `TODO(orchestrator-role)` to go through `is_orchestrator_role`.
- `StartSession`'s `session_role` argument: `StartSessionArgs.session_role: String`, offered in the
  schema as a one-value enum (`orchestrator`), no longer in `INTERNAL_START_FIELDS`; the web parity
  fixture (`start-session-tool-fields.json`) lists it. `into_request` does not carry it yet
  (`TODO(orchestrator-role)`). Two existing "a withheld argument is refused by name" tests
  (`tddy-tool-engine` `start_session_schema.rs`, `tddy-daemon-rpc` `start_session_tool_acceptance.rs`)
  now use `repo_path`, since `session_role` is offered.
- `pub(crate)` in `tddy-agent-launch/src/harness_profile_request.rs`: `RoleRequester<'a>
  { Person, Agent { own_role: Option<&'a str> } }` and `admit_a_requested_role(requester, req)`, whose
  stub delegates to today's `refuse_a_requested_role` (`TODO(orchestrator-role)`).
- Web: test ids `sessions-drawer-item-orchestrator-<id>` (`tddy-session-web` testIds) and
  `create-session-orchestrator-toggle` (`tddy-session-create-web` testIds), rendered nothing yet;
  `CreateSessionOrchestratorField.tsx` (`ORCHESTRATOR_SESSION_ROLE`, props, renders `null`), not wired.
- Test support: `tddy-daemon-rpc/tests/support::a_host_running_claude_as(instance, peers, claude)`.
- Failing acceptance tests below, failing for this node's missing admission/placement/guard.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — every test drives a project-home world or a component of its own
**Concurrent with:** n2 `project-stack-store`, n3 `start-effort`, n4 `session-turn-control`, n5 `session-lifecycle-tools`, n6 `conversation-rotation`
**Blocks:** n7 `stack-plan-tools` (offers its tools by `is_orchestrator_role`), n8 `orchestrator-composer` (starts with `session_role: orchestrator`)

Real dependency edges, as opposed to the branch line:

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] `ORCHESTRATOR_ROLE` + `is_orchestrator_role`; the six exact-string gates call it (launch wiring, read-only guard, `session_tool_role`, owner check, `refuse_a_role_the_agent_cannot_hold`); auto-stop keeps `== PROJECT_HOME_ROLE`
- [ ] `refuse_a_requested_role` admits `orchestrator` from a person and from an orchestrator-role agent; refuses every other agent and any `project-home`
- [ ] Orchestrator placement: reuse `on_home_worktree` + main's type and placement refusals; `sandbox` refused; home worktree fast-forwarded on start and resume
- [ ] Git (worktree-writing) tools refuse a target holding an orchestrator role
- [ ] Exec `StartSession` takes `session_role: orchestrator`
- [ ] ✅ Orchestrator link in the child's initial record ([todo](../todo/2026-10-25-start-session-orchestrator-recorded-after-start.md))
- [ ] Web: Session orchestrator toggle (claude-cli only; hides placement, branch, stack); drawer marker
- [ ] Docs: project-concept, auto-stop, session-drawer via wrap

## Technical Changes

### State A (Current)

- One role, `project-home` (`session_metadata.rs:139`, was :137), compared by exact string at six sites
  (`project_home_launch.rs:255`, `role_guard.rs:44`, `conversation_spawn.rs:25`, `project_home_idle.rs:87`,
  `project_home_remote.rs:146`, `cli_agent_steps/conversation.rs:199`, was :189).
- `refuse_a_requested_role` (`harness_profile_request.rs:128`, was :125) refuses any role on every caller path;
  only `start_session_with_role` (pub(crate), Ensure) bypasses it.
- `on_home_worktree` (`project_home_start_spec.rs:95`) places main; `refuse_what_main_cannot_honour`
  (:24) and `refuse_a_type_main_cannot_run` (:58) check the spec.
- Exec `StartSession` withholds `session_role` (`start_session_args.rs:165`).
- The target resolver refuses only the caller (`session_target.rs:74`): another project-home-role
  session on the shared worktree is a valid git target.
- `record_orchestrator` (`svc_start_session_core/orchestrator_record.rs`, called at
  `svc_start_session_core.rs:144`) rewrites the child's changeset after the start returns.
- Web: no role badge; `PROJECT_HOME_ROLE` used only by `projectHomeGroups.ts`.

### State B (Target)

- `ORCHESTRATOR_ROLE = "orchestrator"`; `is_orchestrator_role` used at five of the six sites; auto-stop
  alone keeps `== PROJECT_HOME_ROLE`.
- `StartSessionRequest.session_role == "orchestrator"` is admitted when the caller principal is a user,
  or an agent (`TokenPrincipal::Agent`) whose session record holds an orchestrator role; the request is
  then placed by the shared main placement before the start path runs.
- A git tool whose resolved target holds an orchestrator role → `is_error` "another orchestrator's
  worktree is the project's home worktree".
- The child's initial record carries the orchestrator link.
- Web toggle and marker.

### Delta

- **tddy-changeset**: constant, predicate, unit tests.
- **tddy-agent-launch**: `refuse_a_requested_role` → `admit_a_requested_role(caller, req)`;
  orchestrator placement shared with `set_up_main`; `project_home_launch.rs` gate; orchestrator link
  written in the initial changeset/metadata; `record_orchestrator` removed.
- **tddy-tool-engine**: `session_tool_role` mapping; `StartSession` schema gains `session_role`.
- **tddy-daemon-rpc**: `role_guard.rs`, `project_home_remote.rs`, git-target refusal.
- **tddy-worktree-service**: `session_target` reports the target's role.
- **tddy-session-create-web**: `CreateSessionOrchestratorField.tsx`; request builder sets `session_role`.
- **tddy-session-web**: marker in `SessionDrawerItem`.

## Implementation Milestones

- [ ] Predicate + gates, auto-stop unchanged (unit + existing project-home suites green)
- [ ] Admission by caller kind
- [ ] Placement + refusals
- [ ] Write-target refusal
- [ ] Start-time orchestrator link
- [ ] Exec `StartSession` argument
- [ ] Web toggle + marker

## Testing Plan

**Level**: integration in `tddy-session-lifecycle`'s project-home world (real daemon start path,
stubbed agent binary), `tddy-daemon-rpc` exec-tool acceptance, Cypress component for web. Cross-host
through the in-memory peer forwarder, never LiveKit
([cheap RPC policy](../todo/2026-11-05-tests-use-cheap-rpc-livekit-only-for-compatibility.md)).

## Acceptance Tests

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/orchestrator_session_acceptance.rs`
- `a_person_starts_an_orchestrator_on_the_home_worktree_read_only` — role recorded before spawn, `repo_path` = home, managed, launched with main's allow/deny lists and `tddy-tools --mcp` only
- `two_orchestrators_and_main_run_on_one_home_worktree`
- `an_orchestrator_naming_a_placement_or_sandbox_is_refused_naming_the_field`
- `an_orchestrator_on_an_agent_without_project_home_main_is_refused`
- `starting_an_orchestrator_fast_forwards_the_home_worktree`
- `idle_auto_stop_leaves_an_orchestrator_running` (beside `project_home_idle_stop_acceptance.rs`)

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/orchestrator_role_exec_tools_acceptance.rs`
- `an_orchestrator_is_offered_mains_tool_set`
- `an_orchestrator_is_refused_a_write_tool_as_main_is`
- `an_ordinary_agent_asking_start_session_for_the_orchestrator_role_is_refused`
- `main_starts_an_orchestrator_through_start_session`
- `a_git_tool_targeting_another_orchestrator_is_refused`
- `a_forwarded_call_from_an_orchestrator_is_served_by_the_other_host`
- `a_session_started_by_an_orchestrator_records_it_before_its_agent_runs`

### tddy-changeset — `packages/tddy-changeset/src/session_metadata.rs` (unit)
- `both_orchestrator_roles_are_orchestrator_roles`, `an_empty_or_unknown_role_is_not`

### tddy-session-create-web — `packages/tddy-session-create-web/cypress/component/CreateSessionOrchestratorAcceptance.cy.tsx`
- `the orchestrator switch sends session_role orchestrator and hides placement, branch and stack`
- `the switch is offered only for an agent that can be held read-only`

### tddy-session-web — `packages/tddy-session-web/cypress/component/SessionsDrawerOrchestratorMarkerAcceptance.cy.tsx`
- `a session with the orchestrator role shows the orchestrator marker`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **A second role value, not reuse of `project-home`.** Reusing it would make auto-stop stop every
  orchestrator and blur "main" (found by `home_session_id`) with "has main's reach". The predicate keeps
  one decision point.
- **"The flag is the role"** (developer, 2026-11-10): no new proto field; `session_role` itself is
  requested. The daemon-only rule now covers `project-home` alone.
- **Agents need the role to grant it** (developer, 2026-11-10): an ordinary agent that can spawn
  conversations would otherwise escalate to terminal keys, answers and pushes across the project.
- **Sandbox refused for orchestrators**: a jailed main gets no tool wiring today (todo above); copying
  that gap to N sessions is worse than refusing.
- **Refuse write targets that are orchestrators**: their worktree is the shared home worktree.

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

n2 `feature/orchestrators/project-stack-store` (the next node in the line).

## TODO

- [x] Record initial discovery (`2026-11-10-orchestrator-role-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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
