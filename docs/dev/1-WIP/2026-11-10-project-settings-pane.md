# Changeset: a project's session settings behind a gear

**Date**: 2026-11-10
**Status**: 🚧 In Progress — draft-PR contract written (pass 2, 6c–6f, 2026-11-14); not committed
**Type**: Feature
**Complexity**: 4 — a proto contract change (`ProjectEntry` + an RPC), a persisted project-record field, four web packages. Re-scored at 6f: unchanged — the new `ProjectData` field fanned out to 72 struct literals in 54 files across 12 crates, but that fan-out is mechanical and already done in the contract
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
- Routes `sessionsProjectSettingsPath(id, {host})` / parser; old setup routes open the same pane
  (in place — see Decisions).
- Composer pre-fill from `ProjectEntry.orchestrator_defaults`.
- Projects card link with host.

## Boundaries

- Does **not** change `EnsureProjectHomeSession`, the pinned entry's click, or the composer's controls.
- Does **not** add harness-profile effort.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n8 `orchestrator-composer` (#885) | `SessionProjectGroupHeader` with an `actions` slot; `OrchestratorComposer` (lists projects itself, `useCreateSessionModel` / `useCreateSessionEffort`); `/sessions/project/:id/new` route | places the gear in the slot; pre-fills the composer's model, effort and provider account from its own `ListProjects` answer | restructure the header, change the composer's controls or its start request |
| n3 `start-effort` (#880) | `CreateSessionEffortField`, effort levels | the defaults form reuses it | change effort |

**Sequencing found at 6f (2026-11-14).** This tree carries n8's **contract** (`cff07d20f`) only, not
its green: `OrchestratorComposer` renders `null`, `SessionProjectGroupHeader` ignores `onNew` and
`actions`, `SessionDrawer` does not read `onNewOrchestrator`, `parseSessionsProjectNew` returns
`null`. n8's green is on `origin/feature/orchestrators/orchestrator-composer` (`660218735`), which is
based on the later nodes' greens, and reaches this branch at the next cascade rebase. So:

- `OrchestratorComposerDefaultsAcceptance` (2 tests) fails **here** on n8's stub (no model select),
  and on this node's missing pre-fill once n8 is green. Verified 2026-11-14 by overlaying n8 green's
  seven `tddy-session-create-web/src` files temporarily: both then fail on this node's pre-fill
  (`expected 'opus' to equal 'sonnet'`; effort `''` vs `'high'`). Real object kept — the composer
  *is* the point.
- The gear test needs n8's header to render `actions`; it fails today on the gear being absent,
  which is this node's own gap too (nothing passes a gear yet).
- No test depends on `TODO(stack-web-by-id)` / `TODO(stack-git-tools)`.
- **Green must also pass `actions` through `SessionProjectGroup`** (n8 green does not:
  `SessionProjectGroup` forwards `onNew` only) — a one-prop change in an n8-touched file.

## Draft PR contract

Written 2026-11-14. Every stub carries `TODO(project-settings-pane): implement`.

**Rust**

- `tddy-projects::project_storage::OrchestratorDefaults { model, effort, provider_account_id }` (each
  `Option<String>`, `skip_serializing_if`), `ProjectData.orchestrator_defaults: Option<…>`;
  `set_project_orchestrator_defaults(projects_dir, project_id, Option<&OrchestratorDefaults>)` —
  stub bails "not implemented yet".
- `project.proto`: `message OrchestratorDefaults { model = 1; effort = 2; provider_account_id = 3 }`
  (empty = agent default), `ProjectEntry.orchestrator_defaults = 12`,
  `rpc SetProjectOrchestratorDefaults(SetProjectOrchestratorDefaultsRequest { session_token = 1;
  project_id = 2; daemon_instance_id = 3; OrchestratorDefaults defaults = 4 })` →
  `SetProjectOrchestratorDefaultsResponse { ProjectEntry project = 1 }`. TS regenerated
  (`tddy-client-web/src/gen`, `tddy-rust-typescript-tests/gen`; `generated-code.sh check` clean).
- `ProjectHandler::set_project_orchestrator_defaults` (required method) + `ProjectServiceImpl`
  adapter + the `Noop` test double in `tddy-projects/src/lib.rs` + `tddy-daemon-rpc` `ports.rs` and
  `test_util.rs`.
- `tddy-daemon-kernel::peer_calls::forward_set_project_orchestrator_defaults` — a thin
  encode/forward/decode wrapper, written in full (no logic).
- `tddy-daemon-rpc/src/project/orchestrator_defaults.rs` (new module):
  `set_project_orchestrator_defaults_at_project_coordinate` — stub `unimplemented`.
  `entries::project_entry_from` sets `orchestrator_defaults: None` (TODO: map the row's).
- `orchestrator_defaults: None` added to every `ProjectData` / proto `ProjectEntry` struct literal
  (72 + 6 sites, 54 files, 12 crates — mechanical).

**Web**

- `tddy-client-web/src/appRoutes.ts`: `PROJECT_SETTINGS_SEGMENT`, `PROJECT_SETTINGS_HOST_PARAM`,
  `sessionsProjectSettingsPath(id, { host? })` (written in full, as n8's path builder was),
  `SessionsProjectSettingsRoute { projectId, host: string | null }`,
  `parseSessionsProjectSettings({ path, params })` — stub `null`. Re-exported from
  `tddy-web/src/routing/appRoutes.ts`.
- `tddy-session-create-web`: `ProjectSettingsPane` + `ProjectSettingsPaneProps` /
  `ProjectSettingsProject` (stub renders `null`); `orchestratorDefaults.ts` —
  `OrchestratorDefaultsChoice`, `NO_ORCHESTRATOR_DEFAULTS`, `ProjectDefaultsRow`,
  `orchestratorDefaultsFor(projects, projectId, host)` (stub: none),
  `orchestratorDefaultsMessage(choice)` (stub: `undefined`). Both exported from `index.ts`.
- `tddy-session-web/src/SessionDrawer.tsx`: prop `onOpenProjectSettings?(projectId)` — not read yet.
- `tddy-workspace-web`: `ProjectMainSessionLink` prop `daemonInstanceId` (not read yet);
  `ProjectCard` passes `group.hosts[0].daemonInstanceId`.

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

Re-verified against `6e785847e` on 2026-11-14.

- `ProjectData` (`tddy-projects/src/project_storage.rs:22`, `home_session_id` :55, setter
  `set_project_home_session` :340); `ProjectEntry` (`project.proto:103`, `reserved 10` :127,
  `home_session_id = 11` :131). ✔ unchanged.
- `ProjectHomeSetupPane` (`tddy-session-create-web/src/ProjectHomeSetupPane.tsx:26`) ✔.
  `ReplaceMainButton` (`tddy-session-web/src/ProjectHomeEntry.tsx:87-106`) ✔.
- **Moved:** routes are no longer one block at `tddy-client-web/src/appRoutes.ts:41-93`. The setup
  route is `:41-56` (`SESSIONS_PROJECT_HOME_SETUP_ROUTE` :41, `sessionsProjectHomeSetupPath` :50);
  n8 and n9 put the project routes between (`SESSIONS_PROJECT_ROUTE` :63, composer :66-81, stack
  view :88-109); the setup parsers moved down to `:127-150` (`parseSessionsProjectHomeSetup` :136);
  `isSessionsDrawerPath` :157.
- **Moved:** `ProjectMainSessionLink` (`tddy-workspace-web/src/projects/ProjectMainSessionLink.tsx`)
  is declared at `:9`; `:18` is the line choosing its path. Its only caller is `ProjectCard.tsx:185`.
- The pane is mounted by `SessionsDrawerScreen.tsx:390-398` from `useProjectHomeSetup` (route read
  `useProjectHomeSetup.ts:24`); the entry's Replace is wired at `SessionsDrawerScreen.tsx:385` and
  `SessionDrawer.tsx:78,262`.
- Per-host project setters route through `ProjectRpcHandler::coordinate_route`
  (`tddy-daemon-rpc/src/project/coordinate_route.rs:25`); `SetProjectAccounts`
  (`coordinate_handlers.rs:404`) is the pattern. `coordinate_handlers.rs` is 564 lines, so the new
  handler is its own module.
- `navigate(path)` (`tddy-client-web/src/ports/navigation.tsx:55`) treats its argument as a path
  only, and `withPath` (`appLocation.ts:110`) **keeps the current params on the same screen** — so a
  `?host=` on a settings route survives a later in-drawer navigation unless green drops it.

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

Written and verified failing 2026-11-14 (fluent-tests style; `mountWithRpc` /
`mountWithClients` + `anInMemoryRpcBackend`, no `cy.intercept`).

### tddy-projects — `packages/tddy-projects/tests/project_orchestrator_defaults_storage.rs`
- `orchestrator_defaults_are_saved_per_host_and_read_back` (:61)
- `a_project_row_without_defaults_reads_as_none` (:82) — a saved row cleared reads as none, no key

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_orchestrator_defaults_rpc_acceptance.rs`
- `set_project_orchestrator_defaults_is_listed_on_the_project_entry` (:228)
- `defaults_set_for_another_host_are_forwarded_there_not_written_here` (:257) — renamed: no peer,
  the forwarding path's own refusal (common room, no room connected) is the observable

### tddy-session-web — `packages/tddy-session-web/cypress/component/SessionsDrawerProjectSettingsAcceptance.cy.tsx`
- `the gear opens the settings pane for that project on the selected host` (:88)
- `main is set up from the settings pane` (:103) — split from "set up and replaced"
- `main is replaced from the settings pane, not from the pinned entry` (:116)
- `the old project-home route opens the settings pane` (:133) — split per route
- `the old replace route opens the settings pane replacing main` (:143)
- `a settings route naming another host acts on that host's main` (:152) — added: PRD AC 5, pane side

### tddy-session-create-web — `packages/tddy-session-create-web/cypress/component/OrchestratorComposerDefaultsAcceptance.cy.tsx`
- `saved defaults pre-fill the composer and can be changed for one start` (:41)
- `a cleared default falls back to the agent's default` (:64)

### tddy-web — `packages/tddy-web/cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx` (new case)
- `a projects card link opens the settings pane for the host its row names` (:177)

### Unit tests (6f)

- `packages/tddy-projects/tests/project_orchestrator_defaults_setter.rs` — replace-not-merge,
  all-absent is none, other fields kept, unknown project errors naming it; one guard
  (`an_older_row_without_the_key_reads_as_none`) passes on the contract.
- `project_orchestrator_defaults_rpc_acceptance.rs` § Handler contract — naming this host saves
  here, absent clears, all-empty lists none, `NotFound`, `InvalidArgument`, `Unauthenticated`
  (one binary with the acceptance tests, to keep the link cost down).
- `packages/tddy-session-create-web/cypress/component/ProjectSettingsPane.cy.tsx` — main section
  (setup form, Replace main, replacing, unknown project) and defaults section (starts from saved,
  saves on the pane's host, all-default clears, refusal shown).
- `packages/tddy-session-create-web/src/orchestratorDefaults.test.ts` — `orchestratorDefaultsFor`
  (host's row only) and `orchestratorDefaultsMessage` (absent when all default).
- `packages/tddy-web/src/routing/appRoutes.test.ts` § sessions project settings route — parse with
  and without `?host=`, encoding round-trip; two guards pass on the contract.

### Closed-world lists and existing tests green must extend

- `packages/tddy-service/tests/unbundle_service_split.rs` `RESIDUAL_METHODS` — **extended in the
  contract** (19 → 20, `SetProjectOrchestratorDefaults`), since the proto lands here.
- `ProjectHandler` implementers — all three updated in the contract.
- `isSessionsDrawerPath` (`tddy-client-web/src/appRoutes.ts`) must accept the settings path.
- Existing specs that pin what this node changes, to be updated (not deleted) at green:
  `tddy-session-web/cypress/component/ProjectHomeReplaceAcceptance.cy.tsx` (Replace on the pinned
  entry; its 4th test asserts `replaceAction` exists), `SessionsDrawerProjectHomeAcceptance.cy.tsx`
  (`onReplaceProjectHome`), the three href cases in
  `tddy-web/cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx` (`#/sessions/project-home/<id>`),
  and any `assertAtSetupFor` whose navigation green moves to the settings path.
- **Cascade hazard:** a node that adds a `ProjectData { … }` literal before this lands breaks the
  build here; `orchestrator_defaults: None` is the fix.

## Technical Debt & Production Readiness

### Baseline before the contract (6c, 2026-11-14) — scoped

- `cargo test -p tddy-projects -p tddy-service`: all green.
- `bun test src`: `tddy-client-web` 144/0, `tddy-workspace-web` 75/0; `tddy-session-create-web`
  7 fail, `tddy-session-web` 2 fail, `tddy-web src/routing` 4 fail — **all inherited contract red**
  of n8 (`buildOrchestratorComposerRequest`, `orchestratorSessionTypeFor`,
  `buildProjectHomeStartSpec`, `withEveryProjectGroup`, composer route) and n9 (stack route). Not
  this node's.
- `tsc` is not a gate (pre-existing errors); the new specs and sources add none of their own.

## Decisions & Trade-offs

- **Defaults on the project record, per host** — main is per host, and the composer starts on a host.
- **Replace main leaves the pinned entry** — the pane is now the one place main is configured.
- **Card link host** — see Prerequisites; open to veto.

Orchestrator decisions (developer: finish without stopping), 2026-11-14:

- **Storage shape** — `ProjectData.orchestrator_defaults: Option<OrchestratorDefaults>`, each control
  `Option<String>`; proto `OrchestratorDefaults` with empty = agent default and message absence =
  none saved. A field on the project record (as planned and as `home_session_id` did), accepting the
  72-literal mechanical fan-out over a sidecar store.
- **Per-host routing** — `SetProjectOrchestratorDefaults` goes to the one host its
  `daemon_instance_id` names via `coordinate_route`; never fanned out to the project's other hosts
  (unlike `SetProjectAccounts`/`SetProjectDefaultBranch`).
- **Old routes open the pane in place**, no URL rewrite — the narrowest reading of the PRD's "keep
  working and open the same pane"; the acceptance tests do not pin a redirect.
- **The gear navigates without `?host=`** (the drawer's selected host); only the Projects card link
  sets `?host=`, and the pane honours it. Replace mode is not in the new URL: the old `/replace`
  route opens the pane with `replacing`.
- **Card link** — only **Set up main** moves to `…/settings?host=<primary host>`; **Open main** stays
  `#/sessions/<id>`.
- **Defaults form** — the "agent default" value is `""` for model and effort and `builtin` for the
  provider; Save sends `defaults` absent when every control is at the default (a clear).
- **Composer pre-fill** reads the composer's own `ListProjects` answer, matching project **and**
  host; a change in the composer is for one start and never saves.
- **Test renames/splits/addition** — see Acceptance Tests.
- **Shared PRD** — `docs/ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md` (n1's) still says
  main is "replaced from the setup pane"; left untouched here (own files only), to be reconciled at
  this node's wrap, the last to touch it.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — skipped by developer instruction 2026-11-14
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
