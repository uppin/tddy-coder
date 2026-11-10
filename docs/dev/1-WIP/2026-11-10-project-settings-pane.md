# Changeset: a project's session settings behind a gear

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Feature
**Complexity**: 4 — a proto contract change (`ProjectEntry` + an RPC), a persisted project-record field, four web packages
**Stack**: `#orchestrators` 11/11 · branch `feature/orchestrators/project-settings-pane` · base `feature/orchestrators/stack-git-tools` (#887)

## Initial Discovery

[2026-11-10-project-settings-pane-initial-discovery.md](./2026-11-10-project-settings-pane-initial-discovery.md)

## Prerequisites

### ✅ RESOLVED HERE — A Projects card's "Set up main" link reflects one host; the setup pane acts on another — [`2026-10-24-project-card-setup-link-ignores-card-host.md`](../todo/2026-10-24-project-card-setup-link-ignores-card-host.md)

The entry left the choice of host to the developer. **Choice made in planning (2026-11-10), open to
veto at the 6e review:** the settings route carries `?host=`, and the card's link passes the host whose
row it read (today the card's primary host). Pinned by the case the entry names in
`packages/tddy-web/cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx`.

### ⚠ DURING — [`oversized-file-create-session-pane.md`](../../../packages/tddy-session-create-web/docs/code-issues/oversized-file-create-session-pane.md)

The pane wraps `ProjectHomeSetupPane`; the defaults form reuses the model/effort/provider fields and
does not extend `CreateSessionPane`.

### ⚠ DURING — Session-web specs do not typecheck — [`2026-11-07-session-web-specs-do-not-typecheck.md`](../todo/2026-11-07-session-web-specs-do-not-typecheck.md)

New specs must typecheck on their own.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10).

## Affected Packages

- [tddy-projects](../../../packages/tddy-projects/) — `ProjectData.orchestrator_defaults`, setter
- [tddy-service](../../../packages/tddy-service/) — `ProjectEntry.orchestrator_defaults`, `SetProjectOrchestratorDefaults`
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — handler, forwarding to the host
- [tddy-session-create-web](../../../packages/tddy-session-create-web/README.md) — `ProjectSettingsPane`, defaults form; composer pre-fill
- [tddy-session-web](../../../packages/tddy-session-web/docs/README.md) — gear on the header; Replace main moves off the entry
- [tddy-client-web](../../../packages/tddy-client-web/README.md) — settings route with `?host=`
- [tddy-workspace-web](../../../packages/tddy-workspace-web/) — Projects card link passes its host

## Related Feature Documentation

- PRD: [PRD-2026-11-10-project-settings-pane.md](../../ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md)
- [Session drawer § Pinned main entry](../../ft/web/session-drawer.md#pinned-main-entry), [Project concept § Data model](../../ft/daemon/project-concept.md#data-model)

## Summary

A gear on each project header opens `#/sessions/project/<id>/settings[?host=]`: main's setup / Replace
main (moved off the pinned entry) and per-project orchestrator defaults (model, effort, provider
account) stored on the project record per host, which the "+" composer pre-fills. Old setup routes open
the same pane; the Projects card link names its host.

## Background

The developer asked for a gear that opens "the project's session settings (today's main setup pane /
project-home agent setting)" (2026-11-10), and chose a settings pane holding the composer defaults.

## Responsibility

- `OrchestratorDefaults { model, effort, provider_account_id }` on `ProjectData` (per host) and
  `ProjectEntry` (new field); `SetProjectOrchestratorDefaults(project_id, daemon_instance_id, defaults)`.
- `ProjectSettingsPane` (main section = `ProjectHomeSetupPane`; defaults section).
- The gear in n8's header slot (`sessions-drawer-project-settings-<id>`); Replace main removed from
  `ProjectHomeEntry`.
- Routes `sessionsProjectSettingsPath(id, {host})` / parser; old setup routes redirect to it.
- Composer pre-fill from `ProjectEntry.orchestrator_defaults`.
- Projects card link with host.

## Boundaries

- Does **not** change `EnsureProjectHomeSession`, the pinned entry's click, or the composer's controls.
- Does **not** add harness-profile effort.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n8 `orchestrator-composer` (#885) | `SessionProjectGroupHeader` with an actions slot; `OrchestratorComposer` and its form state | places the gear in the slot; pre-fills the composer's form | restructure the header, change the composer's controls or its start request |
| n3 `start-effort` (#880) | `CreateSessionEffortField`, effort levels | the defaults form reuses it | change effort |

## Draft PR contract

- Proto field + RPC + gen; `OrchestratorDefaults` type; handler stub (`TODO(project-settings-pane)`);
  `ProjectSettingsPane` shell; route helpers.
- Failing tests below.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — needs n8's header slot and composer
**Concurrent with:** n10
**Blocks:** none

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] Defaults on the project record + RPC + listing
- [ ] Settings pane (main section + defaults)
- [ ] Gear; Replace main moves into the pane
- [ ] Routes with `?host=`; old routes open the pane
- [ ] Composer pre-fill
- [ ] ✅ Projects card link host ([todo](../todo/2026-10-24-project-card-setup-link-ignores-card-host.md))

## Technical Changes

### State A (Current)

- `ProjectData` (`tddy-projects/src/project_storage.rs:22`) has `home_session_id`; `ProjectEntry`
  (`project.proto:103`, `home_session_id = 11`, 10 reserved).
- `ProjectHomeSetupPane` (`tddy-session-create-web/src/ProjectHomeSetupPane.tsx:26`) hosts the main
  variant of `CreateSessionPane`; routes `tddy-client-web/src/appRoutes.ts:41-93`;
  `ReplaceMainButton` (`tddy-session-web/src/ProjectHomeEntry.tsx:87-106`).
- `ProjectMainSessionLink` (`tddy-workspace-web/src/projects/ProjectMainSessionLink.tsx:18`).

### State B (Target)

- As Responsibility.

### Delta

- projects store + setter; proto; daemon handler; create-web pane + composer pre-fill; session-web gear
  + entry; client-web routes; workspace-web link.

## Implementation Milestones

- [ ] Store + RPC
- [ ] Pane + routes
- [ ] Gear + Replace move
- [ ] Composer pre-fill
- [ ] Card link

## Testing Plan

**Level**: `tddy-projects` storage unit; daemon RPC acceptance; Cypress component for pane, gear,
composer pre-fill and the card link (`mountWithRpc` + `anInMemoryRpcBackend`).

## Acceptance Tests

### tddy-projects — `packages/tddy-projects/tests/project_orchestrator_defaults_storage.rs`
- `orchestrator_defaults_are_saved_per_host_and_read_back`
- `a_project_row_without_defaults_reads_as_none`

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_orchestrator_defaults_rpc_acceptance.rs`
- `set_project_orchestrator_defaults_is_listed_on_the_project_entry`
- `defaults_set_for_another_host_are_written_there`

### tddy-session-web — `packages/tddy-session-web/cypress/component/SessionsDrawerProjectSettingsAcceptance.cy.tsx`
- `the gear opens the settings pane for that project on the selected host`
- `main is set up and replaced from the settings pane`
- `the old project-home routes open the settings pane`

### tddy-session-create-web — `packages/tddy-session-create-web/cypress/component/OrchestratorComposerDefaultsAcceptance.cy.tsx`
- `saved defaults pre-fill the composer and can be changed for one start`
- `a cleared default falls back to the agent's default`

### tddy-web — `packages/tddy-web/cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx` (new case)
- `a projects card link opens the settings pane for the host its row names`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Defaults on the project record, per host** — main is per host, and the composer starts on a host.
- **Replace main leaves the pinned entry** — the pane is now the one place main is configured.
- **Card link host** — see Prerequisites; open to veto.

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

None — the top of the stack.

## TODO

- [x] Record initial discovery (`2026-11-10-project-settings-pane-initial-discovery.md`)
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
