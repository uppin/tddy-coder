# Changeset: Workspace screens package (`tddy-workspace-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages (new package; app routes and sessions' worktree tab), ~56 specs moved, a cycle cut, an inherited oversized-file record

**Stack**: `#web-split` 12/16 — branch `feature/web-split/workspace`, base `feature/web-split/hosts`

**PR**: [#849](https://github.com/uppin/tddy-coder/pull/849)

## Initial Discovery

[2026-11-07-workspace-package-initial-discovery.md](./2026-11-07-workspace-package-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) and `packages/*/docs/code-issues/` this change runs into.

### ⚠ DURING — oversized `useModelRegistryFanOut` — `packages/tddy-web/docs/code-issues/oversized-file-use-model-registry-fan-out.md`; [`2026-09-28-web-model-fan-out-over-budget.md`](../todo/2026-09-28-web-model-fan-out-over-budget.md)

Moves with the file; recorded, not fixed here.

## Affected Packages

- **tddy-workspace-web** (new): `packages/tddy-workspace-web/` — sources, Cypress specs, page objects/drivers/fixtures, README, `docs/`, moved code-issue records.
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — the moved modules leave; consumers import from `tddy-workspace-web`; feature docs under `packages/tddy-web/docs/` for this area move to `packages/tddy-workspace-web/docs/` at wrap.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-workspace-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-workspace-package.md](../../ft/web/1-WIP/PRD-2026-11-07-workspace-package.md)

## Summary

Move accounts, projects, settings, models/agents, tasks, worktrees and the sign-in dialogs into one workspace-screens package.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-workspace-web` exports `AccountsScreen`, `ProjectsScreen`, `DaemonSettingsScreen`, `ModelsScreen` (+ `useModelRegistryFanOut`, agent pickers), `TasksScreen`, `WorktreesScreen`, `SessionWorktreeTab`, `worktreeStatsFormat`, `CodexOAuthDialog`, `DeviceLoginPanel`, `CredentialVaultPrompt`, `DaemonLoginScreen`, `GitHubLoginButton`, `UserAvatar`, and their hooks/helpers (`lib/{worktreeList,worktreeSize,codexOauthMetadata,vaultPassphrase}`).
- Moved specs (~56: `Account*`, `Project*`, `Provider*`, `Daemon*Settings*`, `Task*`, `Worktree*` except the code pane, `CodexOAuth*`, `DeviceLogin*`, `CredentialVault*`, `RedirectLogin*`, `AuthProviderRefresh*`, `StandaloneFormTheme*`, `cypress/component/{models,tasks}/`) and their page objects.
- Its boundary rule test `packages/tddy-workspace-web/src/packageBoundary.test.ts` and `web-tddy-workspace-web.yml`.

## Boundaries

- `*AppPage` wrappers and `AuthCallback` stay in `tddy-web` (wiring); auth's implementation stays behind `AuthPort`.
- `WorktreeCodePane` stays with sessions (code pane).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, theme, shared helpers | renders with them | move UI primitives |
| `n5` clients (#842) | generated clients, `ClientsPort`/`NavigationPort`/`AuthPort` hooks, `mountWithClients` | all RPC/navigation/auth through them; specs mount with `mountWithClients` | add a transport or provider |
| `n10` chat | `tddy-chat-web` components | models renders chat from it | move chat |
| `n11` hosts | — (branch predecessor only) | nothing | — |

## Draft PR contract

**As pushed (commit 2):** skeleton, workflow, boundary rule; **40** specs (top-level workspace specs and
`cypress/component/{models,tasks}/`) whose only session imports move with this package, `git mv`'d —
failing until the modules move. Daemon-*selector* specs and project-home specs are not workspace screens and
stay for `feature/web-split/session`/`wiring`.

Originally planned:

First push (commit 2): the `tddy-workspace-web` skeleton (manifest exporting the surface in `## Responsibility`, Cypress/Vite config, workflow), the specs, page objects and drivers listed below `git mv`'d into the package with imports pointing at it, and the boundary rule — failing until the modules move.

## Green wave

**Wave:** 6 of 9
**Greenable independently:** yes — specs mount the screens against in-memory daemon RPC; models' chat comes from the published chat package
**Concurrent with:** `prstack`
**Blocks:** `session` (worktree tab)

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] `tddy-workspace-web` skeleton + workflow
- [ ] Modules moved; `tddy-web` (and sibling packages) import from `tddy-workspace-web`
- [ ] Specs, page objects, drivers moved and green from the package config
- [ ] Boundary rule
- [ ] Code-issue records and package docs moved

## Technical Changes

### State A (Current)

`components/{accounts (1.2k), projects (1.0k), settings (0.4k), models (2.4k), tasks (0.4k), worktrees (0.3k)}` + root sign-in components; worktrees imports `sessions/{SessionWorktreeTab,worktreeStatsFormat}`; models imports chat; `useModelRegistryFanOut` recorded oversized.

### State B (Target)

`tddy-workspace-web` owns them; app routes and sessions import it.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [ ] `tddy-workspace-web` specs green
- [ ] `tddy-web` builds; consumers' scoped specs green

## Testing Plan

**Level:** Cypress component from the package's own config (existing assertions, mount helper swapped to `mountWithClients` + port doubles), `bun test` for moved pure suites, a boundary rule.

## Acceptance Tests

### tddy-workspace-web

- Moved: the ~36 top-level specs listed in `## Responsibility` and the 20 under `cypress/component/{models,tasks}/` — unchanged assertions.
- `src/packageBoundary.test.ts` — `tddy-workspace-web imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **One package for six small screens** (developer's granularity choice) — each is one daemon service's list/form.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-workspace-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`tddy-workspace-web` Cypress + `bun test` + boundary rule; `tddy-web` build + consumers' scoped specs) — verify 100% pass
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
- [ ] Linting and formatting
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
