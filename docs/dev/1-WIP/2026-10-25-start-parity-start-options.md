# Changeset: DescribeSessionStartOptions lists a host's agents, models and choices

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — two crates, an adapter seam onto the catalog, accounts and host RPCs
**Stack**: `#start-parity` 5/9 — branch `feature/start-parity/start-options`, base `n4` `async-start` (`feature/start-parity/async-start`)
**PRD**: [PRD-2026-10-25-start-parity-start-options.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-start-options.md)

## Initial Discovery

See [./2026-10-25-start-parity-start-options-initial-discovery.md](./2026-10-25-start-parity-start-options-initial-discovery.md).

## Responsibility

- The discovery tool: its def, port and daemon implementation.
- The Rust mirror of the stack-parent filters, with a parity fixture.

## Boundaries

- Does not change `StartSession`'s schema. The host enum stays the `start-tool` node's.
- Adds no RPC.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` `remote-targets` (`feature/start-parity/remote-targets`) | the delegated home caller, the `daemon_instance_id` target argument, and the forward of a whole project-home tool call to the target's host | its tool takes `daemon_instance_id` and is forwarded by n1's route; one cross-host test drives it | add a forward, a delegated-caller check, or a per-tool peer call |

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

`DESCRIBE_SESSION_START_OPTIONS` def, `StartOptions`, `ProjectHomePort::start_options`, the fixture `stack-parent-candidates.json`, and failing tests.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its tests exercise `n1` `remote-targets` (`feature/start-parity/remote-targets`)'s behaviour, which must be green first
**Concurrent with:** `n3` `worktree-list` (`feature/start-parity/worktree-list`), `n4` `async-start` (`feature/start-parity/async-start`), `n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`)
**Blocks:** nothing

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

`n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`), `n8` `disk-refresh` (`feature/start-parity/disk-refresh`), `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

## Affected Packages

- [ ] `tddy-tool-engine`
- [ ] `tddy-daemon-rpc`

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-start-options.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-start-options.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

Several of `StartSession`'s choices depend on the host and on each other: agents, models per
agent and provider account, provider routes, SSH hosts, subagents, branches, and stack-parent
candidates. The form fetches them live. A static schema cannot hold them without going stale, so
this node adds **`DescribeSessionStartOptions`**, which answers them from the same RPCs the form uses.

## Scope

- [ ] An orchestrator can find the agents, models, provider accounts, SSH hosts, subagents, branches and stack parents it may choose for a start on any host, the same ones the form offers.

## Technical Changes

### State A

The form's sources:
- agents: `CatalogService.ListAgents` (`tddy-daemon-rpc/src/catalog/agent_rows.rs`);
- models: `ListAgentModels{agent}` (cached, shells out to `tddy-tools list-models`) or
  `AccountsService.ListProviderAccountModels`;
- provider routes: `ListProviderRoutes{harness}`;
- SSH hosts: `HostService.ListSshConfigHosts`;
- subagents: `CatalogService.ListSubagents`;
- branches: `ProjectService.ListProjectBranches`;
- stack parents: `ListSessions` filtered (`tddy-web/src/utils/stackParents.ts`).

No tool exposes any of these.

### State B

`DescribeSessionStartOptions {daemon_instance_id?, session_type?, agent?, provider_account_id?}`
→ `{agents, models: {default, choices}, provider_routes, ssh_config_hosts, subagents, branches,
stack_parents, pr_stack_base_candidates}`.
- Each list comes from the RPC the form calls, run on the named host. A remote host is reached
  through `remote-targets`' forward.
- The stack filters (`stackParents.ts`) are mirrored in Rust and pinned by a fixture shared with a
  Cypress spec.

## Testing Plan

### Acceptance Tests

- [ ] **Integration** `packages/tddy-daemon-rpc/tests/start_options_acceptance.rs` — `models_are_the_catalogs_models_for_the_chosen_agent`: Equal to `ListAgentModels{agent}` on the same host, default included.
- [ ] **Integration** `same file` — `agents_subagents_and_ssh_hosts_equal_the_forms_sources`: Each equals its RPC's answer.
- [ ] **Integration** `same file` — `a_provider_account_lists_that_accounts_models`: With `provider_account_id` set: `ListProviderAccountModels`.
- [ ] **Cypress component** `packages/tddy-web/cypress/component/stackParentsParity.cy.ts` — `the tool offers the same stack parents and stack bases as the form`: The TS filters over the fixture sessions equal the Rust output.
- [ ] **Integration** `packages/tddy-daemon/tests/project_home_remote_targets_acceptance.rs` — `start_options_describe_another_hosts_choices`: Agents come from the peer's catalog.

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests use the
in-memory peer forwarder if it exists on the base, and the LiveKit testkit otherwise.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **A discovery tool rather than enums** for per-host, per-agent lists. A schema frozen at `ListExecTools` time would go stale, and computing models there would make the catalog call slow.

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

- [x] Record initial discovery (`2026-10-25-start-parity-start-options-initial-discovery.md`)
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
