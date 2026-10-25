# Changeset: StartSession takes every New-session option, on any host

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto change (session.proto), four packages including tddy-web, parity against the proto descriptor and the UI builder, orchestrator tag moved into the start core
**Stack**: `#start-parity` 2/9 — branch `feature/start-parity/start-tool`, base `n1` `remote-targets` (`feature/start-parity/remote-targets`)
**PRD**: [PRD-2026-10-25-start-parity-start-tool.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-start-tool.md)

## Initial Discovery

See [./2026-10-25-start-parity-start-tool-initial-discovery.md](./2026-10-25-start-parity-start-tool-initial-discovery.md).

## Prerequisites

- ⚠ [2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md](../todo/2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md) — the remote-start test uses the in-memory forwarder if it has landed.

## Responsibility

- The args model, the internal-field table, and the schema generator.
- The `StartSession` exec tool and its full field mapping onto `StartSessionRequest`.
- The orchestrator proto fields, and recording them in the start core.
- Both parity tests, with the Rust-written schema-fields fixture the Cypress spec reads.

## Boundaries

- Does not make the start asynchronous, and adds no status or cancel tools (that is `async-start`).
- Does not add the per-host discovery lists (`start-options`).
- Does not touch `spawn_conversation` (`spawn-conversation`).
- Does not change the form's behaviour. The form's drift (8 recipes vs. the server's 10) is
  recorded as a TODO.

## Dependencies

None — this node is a root of the dependency graph. It still sits on its line predecessor's branch
(the flattening), but consumes nothing from it.

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

- `StartSessionArgs`, `INTERNAL_START_FIELDS`, `start_session_schema(hosts: &[HostChoice]) -> Value`.
- `START_SESSION` replaces `START_PROJECT_SESSION`.
- `session.proto` fields 44 and 45.
- Fixture `packages/tddy-web/cypress/fixtures/start-session-tool-fields.json`, written by
  `cargo test -p tddy-tool-engine` and checked for staleness by it.
- Failing tests (below).

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — its tests need only its own surface and today's master
**Concurrent with:** `n1` `remote-targets` (`feature/start-parity/remote-targets`)
**Blocks:** `n4` `async-start` (`feature/start-parity/async-start`), `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

`n3` `worktree-list` (`feature/start-parity/worktree-list`), `n4` `async-start` (`feature/start-parity/async-start`), `n5` `start-options` (`feature/start-parity/start-options`), `n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`), `n8` `disk-refresh` (`feature/start-parity/disk-refresh`), `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

## Affected Packages

- [ ] `tddy-service (session.proto)`
- [ ] `tddy-tool-engine`
- [ ] `tddy-daemon-rpc`
- [ ] `tddy-agent-launch`
- [ ] `tddy-web (parity spec only)`

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-start-tool.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-start-tool.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

`StartProjectSession` exposes five of the New-session form's ~30 options and can only start on
its own host. This node replaces it with **`StartSession`**. The tool's arguments are
`StartSessionRequest`'s own fields, its schema is generated, and two parity tests keep it in step
with the proto and with what the form can send. The orchestrator link moves into the request, so
the daemon that creates the session records it, on any host.

## Scope

- [ ] An orchestrator can set every option the New-session form offers.
- [ ] The schema lists closed choices as enums and says which fields each session type requires.
- [ ] A start naming another eligible host creates the session there, recorded as started by the orchestrator.
- [ ] A new request field, or a new form option, the tool lacks fails a test.

## Technical Changes

### State A

- `StartProjectSessionArgs` {session_type, prompt, agent, new_branch_name, model}. The schema
  is hand-written in `start_project_session_schema()`
  (`tddy-tool-engine/src/project_home_tools.rs`).
- `DaemonProjectHome::start_session` sends a unary `StartSession` with `daemon_instance_id`
  empty. `tag_orchestrator` then writes `orchestrator_session_id` into the new session's
  changeset **on this host** (`tddy-daemon-rpc/src/exec_tool/project_home.rs:297-328,381`).
- The form builds its request in `buildStartSessionRequest`
  (`tddy-web/src/components/sessions/createSessionRequest.ts:69-229`); it has no
  machine-readable option list.

### State B

- `tddy-tool-engine/src/start_session_args.rs`: `StartSessionArgs`, whose serde names are
  `StartSessionRequest`'s field names.
  - It has every field the form can send.
  - `attachments` covers host documents only.
  - `project_id` and `session_token` are filled in by the caller's context.
  - `on_branch_conflict` is fixed to `"reject"`.
  - `INTERNAL_START_FIELDS` names each proto field the tool withholds, with a reason.
- The schema is generated from the args model:
  - enums for session types, recipes (`supported_workflow_recipe_cli_names()`), permission modes
    and branch intents;
  - `if/then` blocks that state required-ness per session type and per intent;
  - a description on every property.
- The host enum is filled in when `ListExecTools` answers, from the eligible-daemon roster.
- New `StartSessionRequest` fields: `orchestrator_session_id = 44` and
  `orchestrator_daemon_instance_id = 45`. The start core records them in the new changeset on the
  daemon that creates the session, so a forwarded start is tagged on the peer. `tag_orchestrator`
  is removed.
- The tool is renamed `StartProjectSession` → `StartSession`, with no alias. It is still
  synchronous here; `async-start` makes it return a handle.

## Testing Plan

### Acceptance Tests

- [ ] **Unit** `packages/tddy-tool-engine/tests/start_session_args_parity.rs` — `every_start_session_request_field_is_offered_or_named_internal_with_a_reason`: Decode `SERVICE_DESCRIPTOR_BYTES`. The fields of `session.StartSessionRequest` equal (schema properties ∪ internal table ∪ context-filled fields) exactly. The failure message names the missing field.
- [ ] **Unit** `same file` — `the_internal_table_names_only_real_fields`: Every internal-table key is a proto field.
- [ ] **Unit** `same file` — `the_web_fixture_lists_exactly_the_offered_fields`: The fixture JSON equals the schema's property list (regen hint on failure).
- [ ] **Unit** `packages/tddy-tool-engine/tests/start_session_schema.rs` — `recipe_enum_is_the_servers_supported_recipes`: Equals `supported_workflow_recipe_cli_names()`.
- [ ] **Unit** `same file` — `model_is_required_exactly_for_harnesses_that_need_one`: The `if/then` covers exactly the `cli_harness_requires_model` types.
- [ ] **Unit** `same file` — `every_property_is_described`: Each property has a non-empty description.
- [ ] **Cypress component** `packages/tddy-web/cypress/component/createSessionRequestParity.cy.ts` — `every request field the New-session form can send is offered by the StartSession tool`: Run `buildStartSessionRequest` for all six session types with every flag on. Every set key (camelCase → proto name) is in the fixture.
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/start_session_tool_mapping.rs` — `start_session_passes_every_offered_field_to_the_start_path`: Recording launch port: each field arrives with the value sent.
- [ ] **Integration** `packages/tddy-daemon/tests/project_home_start_remote_acceptance.rs` — `start_session_on_another_host_creates_the_session_there_tagged_with_the_orchestrator`: The peer's session dir exists. The peer's changeset has `orchestrator_session_id` = the home id. Nothing is created locally.
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/start_session_tool_mapping.rs` — `the_host_enum_offered_is_the_eligible_daemon_roster`: The `ListExecTools` schema's `daemon_instance_id` enum equals the roster plus `""`.

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests use the
in-memory peer forwarder if it exists on the base, and the LiveKit testkit otherwise.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **The proto is the source of truth, not the form model.** The form model is a TS interface with
  derived and renamed keys; the parity tests anchor on the descriptor and on the builder's
  output.
- **`prompt` is renamed `initial_prompt`**, with no alias. The tool is days old.
- **The bypass fields are exposed** (developer, Q1): an orchestrator may start a child with
  `dangerously_skip_permissions`.

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |

## TODO

- [x] Record initial discovery (`2026-10-25-start-parity-start-tool-initial-discovery.md`)
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
