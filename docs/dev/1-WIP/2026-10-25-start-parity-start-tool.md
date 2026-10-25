# Changeset: StartSession takes every New-session option, on any host

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto change (session.proto), four packages including tddy-web, parity against the proto descriptor and the UI builder, orchestrator tag moved into the start core
**PR**: [#751](https://github.com/uppin/tddy-coder/pull/751)
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

- `session.proto`: `StartSessionRequest.orchestrator_session_id = 44` and
  `orchestrator_daemon_instance_id = 45`. No literal in the workspace lists every field, so
  nothing else changes.
- `tddy-tool-engine::start_session_args`:
  - `START_SESSION`, `StartSessionArgs` (`deny_unknown_fields`) and `HostDocumentAttachment`;
  - `CONTEXT_FILLED_FIELDS` and `INTERNAL_START_FIELDS`, each entry with its reason;
  - `HostChoice`, `StartChoices { hosts, recipes }` and `StartContext`;
  - `start_session_schema(&StartChoices)` and `StartSessionArgs::into_request(StartContext)`,
    both `TODO(start-tool)`;
  - `offered_start_fields`, which is derived from the schema.
- The recipe list is passed in by the daemon (`StartChoices.recipes`). That keeps
  `tddy-workflow-recipes` and its tree (tddy-github, tddy-pr-stack, tddy-workflow) out of the tool
  engine.
- Fixture `packages/tddy-web/cypress/support/fixtures/start-session-tool-fields.json`
  (`{offered, context_filled}`), committed with the **target** list. The Rust test keeps it equal
  to the schema.
- `packages/tddy-daemon-rpc/tests/support/mod.rs`: the two-host in-process world, shared with later
  nodes' cross-host tests.
- New dev-dependencies of `tddy-tool-engine`: `prost` and `prost-types`. Both are already in the
  workspace; they decode the compiled-in descriptor.

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

- [ ] **Unit** `packages/tddy-tool-engine/tests/start_session_args_parity.rs`:
  - `every_start_session_request_field_is_offered_context_filled_or_withheld`
  - `the_tool_offers_no_argument_the_proto_does_not_have`
  - `the_context_and_withheld_tables_name_only_real_fields_and_never_an_offered_one`
  - `the_web_fixture_lists_exactly_the_offered_and_context_filled_fields`
- [ ] **Unit** `packages/tddy-tool-engine/tests/start_session_schema.rs`:
  - `session_type_offers_tool_workspace_and_every_registered_cli_harness`
  - `the_host_is_this_host_or_any_eligible_host`
  - `the_recipe_is_one_the_daemon_supports_or_none`
  - `permission_mode_and_branch_intent_are_their_closed_lists`
  - `model_is_required_exactly_for_the_harnesses_that_need_one`
  - `working_on_a_selected_branch_requires_naming_it`
  - `every_argument_is_described_and_only_session_type_is_always_required`
  - `every_argument_reaches_the_request_under_its_own_name`
  - `a_host_document_attachment_reaches_the_request_as_one`
  - `an_argument_the_tool_does_not_offer_is_refused_by_name`
- [ ] **Cypress component**
  `packages/tddy-web/cypress/component/createSessionRequestParity.cy.ts` — "offers every request
  field the New-session form can send". It runs every session type × the three codebase
  placements with every option set and a host-document attachment.
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/start_session_tool_acceptance.rs` (two
  in-process daemons):
  - `start_session_is_offered_with_every_eligible_host_and_the_daemons_recipes`
  - `start_session_starts_on_the_branch_it_names_linked_to_the_orchestrator`
  - `start_session_on_another_host_creates_the_session_there_linked_to_the_orchestrator`
  - `a_branch_another_session_owns_is_reported_not_suffixed`
  - `an_argument_the_form_does_not_offer_is_refused_by_name`

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests run in the two-host world of `tests/support/mod.rs`.

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
- [ ] `#750`'s `project_home_remote_targets_acceptance.rs` carries its own copy of the two-host world that `tests/support/mod.rs` now holds. Switch it to `mod support;` when that node greens; this node does not edit #750's file.
- [ ] The #749 test `tests/start_project_session_schema.rs` and the `StartProjectSession` cases in `project_home_session_tools_acceptance.rs` move to `StartSession` with the rename.
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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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
