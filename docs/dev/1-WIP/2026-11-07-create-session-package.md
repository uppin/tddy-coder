# Changeset: Create-session package (`tddy-session-create-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress — green phase (implementation + moved specs)
**Type**: Architecture Change
**Complexity**: 4 — two packages (new package, `tddy-web` consumers incl. prstack's start-session), ~44 specs + page objects moved, an inherited oversized-file record

**Stack**: `#web-split` 7/16 — branch `feature/web-split/create-session`, base `feature/web-split/livekit`

**PR**: [#844](https://github.com/uppin/tddy-coder/pull/844)

## Initial Discovery

[2026-11-07-create-session-package-initial-discovery.md](./2026-11-07-create-session-package-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) and `packages/*/docs/code-issues/` this change runs into.

### ⚠ DURING — oversized `CreateSessionPane.tsx` — `packages/tddy-web/docs/code-issues/oversized-file-create-session-pane.md`

Moves with the file to `packages/tddy-session-create-web/docs/code-issues/`; the measurement is unchanged by a move. Recorded, not fixed here.

## Affected Packages

- **tddy-session-create-web** (new): `packages/tddy-session-create-web/` — sources, specs, page objects (`createSessionPage`, `branchConflictDialogPage`, …), fixtures (`start-session-tool-fields.json`, `stack-parent-candidates.json`), README, `docs/`, `docs/code-issues/oversized-file-create-session-pane.md` (moved).
- **tddy-web**: loses `components/sessions/{CreateSession*,createSession*,useCreateSession*,codebasePlacement,cliHarnessOptions,selectableAgentOptions,useAvailableAgents,useProjectBranches,useSpecializedAgentPicks,useProjectHomeStartSubmit,projectHomeSessionTypes,contextDocPath,codingAgentMetadata,startProgress,BranchConflictDialog,CreateSessionStartProgress,StartPassphrasePrompt}`, `sessions/attachments/`, `sessions/prstack/baseBranchChoice`, `hooks/{useSessionAttachments,useStagedAttachmentUpload}`, `lib/{attachmentBasenames,attachmentBytes,branchConflict,branchNames,projectSelectOptions}` (final list by consumer count in pass 2); consumers import from the package; `docs/create-session-provider.md`, `start-passphrase-prompt.md`, `session-start-and-indexing-progress.md` at wrap.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-session-create-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-create-session-package.md](../../ft/web/1-WIP/PRD-2026-11-07-create-session-package.md)

## Summary

Move the whole create-session flow — pane, dialog, fields, hooks, request builder, attachment staging, start progress — into its own package.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-session-create-web` exports `CreateSessionPane`, `CreateSessionDialog`, `CreateSessionStartProgress`, `BranchConflictDialog`, `StartPassphrasePrompt`, the hooks other packages call (`useCreateSessionSubmit`, `useAvailableAgents`, `useProjectBranches`, `useSessionAttachments`), `buildCreateSessionRequest`, `baseBranchChoice`, and the attachment components (`SessionAttachmentList`, `HostDocumentPicker`, `pendingAttachment`).
- The moved Cypress specs (the ~44 `CreateSession*`, `*Attachment*`, `CodebasePlacementRules`, `createSessionRequestParity`, `stackParentsParity`, `SessionStartAndIndexingProgress`, `ProjectHomeSetup*` specs), their page objects and fixtures.
- Its boundary rule test and workflow; the moved code-issue record.

## Boundaries

- Does not move the session runtime, drawer or inspector (`feature/web-split/session`), PR-stack screens (`feature/web-split/prstack`), or project-home *entry* in the drawer.
- Does not split `CreateSessionPane.tsx` (the oversized-file record moves with it; splitting it is not this node's change).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, `cn`, theme | renders with them | move primitives |
| `n5` clients (#842) | generated clients, `ClientsPort` hooks (`useDaemonClient`, `useHostFanOut`, `useHostClient`), `NavigationPort`, `mountWithClients` | all RPC and navigation goes through them; specs mount with `mountWithClients` | add a transport or a provider |
| `n6` livekit (#843) | — (branch predecessor; create-session has no LiveKit reach) | nothing | — |

## Draft PR contract

**As pushed (commit 2):** package skeleton (manifest, tsconfig, Vite/Cypress config, support with
`cy.mountWithClients` from `tddy-client-web/testkit` and the `tddy-ui-web` theme), `web-tddy-session-create-web.yml`,
the boundary rule (imports + exported surface), and **38** specs `git mv`'d with their `src/components/sessions/`
imports re-pointed at the package's `src/`. Five specs that mount `SessionsDrawerScreen` (or the usage tab)
to exercise create-session through the drawer stay in `tddy-web` and travel with `feature/web-split/session`:
`CreateSessionAcceptance`, `CreateSessionAndStreamsVisibleErrorsAcceptance`, `CreateSessionAutoClosesDrawer`,
`CreateSessionBranchConflictAcceptance`, `ProjectHomeSetupPaneAcceptance`. Moved specs still import
`tddy-web` support (`lib/participantRole`, `rpc/selectedDaemon`, page objects, `testIds`) — green replaces
those with `tddy-client-web` ports/testkit and package-local support.

Originally planned:

First push (commit 2): the package skeleton (manifest exporting the surface above, Cypress/Vite config, workflow), the ~44 specs `git mv`'d into `packages/tddy-session-create-web/cypress/component/` with imports pointing at the package, and its boundary rule test — failing until the modules move.

## Deviations (green phase)

Recorded where the red-phase contract met the code. Nothing here widens scope.

- **`buildCreateSessionRequest` does not exist** — the request builder is `buildStartSessionRequest` (and `buildProjectHomeStartSpec`). The red boundary test and the Responsibility list named a fabricated symbol; the test and surface use the real name.
- **Three more specs stay in `tddy-web`** (38 moved in red; 35 live in the package, one of them split):
  `CreateSessionClonePassphraseAcceptance` and `CreateSessionClonePassphraseMount` pin the Hosts screen's
  passphrase dialog (`HostPassphraseDialog`, `useHostPromptAnswer`, RSA-OAEP encryption, host-key pinning),
  which `feature/web-split/hosts` owns; `ProjectHomeSetupLinkAcceptance` mounts `SessionDrawer` and the Projects
  page. `SessionStartAndIndexingProgress` is split: its create-pane test stays in the package, its
  session-header indexing test is `tddy-web`'s `SessionIndexingIndicatorAcceptance` (same assertions).
- **`StartPassphrasePrompt` is a slot, not the dialog.** The package exports `StartPassphraseProvider` and a
  `StartPassphrasePrompt` that mounts whatever component the host application supplies (and throws without a
  provider — a clone asking for a passphrase nobody can answer would hang silently). `tddy-web` supplies
  `HostStartPassphrasePrompt` (the old component, moved to `components/hosts/`) via `HostStartPassphraseScope`
  around every create-session surface and in its Cypress `mount`.
- **`baseBranchChoice()` stays with prstack.** It resolves a `StackNode` (`stackPlan`, `deriveStackBaseBranch`,
  `prioritiseBaseBranchOptions`), which are the PR-stack screen's; create-session only ever used its `BaseBranchOption`
  type. The package owns `BaseBranchOption` / `BaseBranchChoice` (`baseBranchOptions.ts`); `prstack/baseBranchChoice.ts`
  re-exports them and keeps the function.
- **Moved to `tddy-ui-web`:** `safeTestIdPart` (`testId.ts`, with its test) and `WorktreeFileTree` (its own minimal
  `WorktreeFilesApi { listDir }`). **Moved to `tddy-client-web`:** `rpcFailure` (+ test), `randomUuid` (+ test),
  `fileUploadChunks` (+ `UPLOAD_CHUNK_TIMEOUT_MS`) and `worktreeFilesApi`. All were shared with code that stays in
  `tddy-web`; `tddy-web` imports them from the packages (no stubs).
- **Also moved into the package** because only create-session used them: `useAgentModels`, `useProviderRoutes`,
  `utils/stackParents`, `lib/{branchConflict,branchNames,projectSelectOptions,attachmentBasenames,attachmentBytes}`,
  `agentPlacementChoice`, `AgentKindBadge`, `useSelectableAgents` (the drawer and the agent picker import them back from
  the package).
- **`CreateSessionSshConfigSelect` no longer reads `useLiveKitTransportFactoryIsOverridden`** (a test-only escape hatch in
  production code); it is reachable when the host connection reports `connected`, which the clients-port double does.
- **Package support is a copy where `tddy-web` keeps using the original** (`servedAgents`, `rpc/{responses,protoRpc,connectionRpcs,sessionFilesServiceBackend,sessionAgentRosterBackend,recordedRequests}`,
  `util/fileDrop`, `pages/{createSessionPage,projectHomeSetupPage}`) — TODO: dedupe into a shared testkit when `session` moves.
  `testIds.ts` is trimmed to the entries these specs use. `fixtures/*.json` and `branchConflictDialogPage` moved.
- `cy.mountWithClients` + `mounts.tsx` (`mountWithHostBackends`: one backend per host, replacing `mountWithPerDaemonLiveKitRpc`) replace
  `SelectedDaemonProvider`/`mountWithRpc`; targets are host ids, not LiveKit identities.
- `lucide-react` and `@connectrpc/connect-web` (dev) added to the package manifest — already workspace dependencies.

## Green wave

**Wave:** 4 of 9
**Greenable independently:** yes — the specs mount create-session against `mountWithClients` + in-memory backends; nothing from n6
**Concurrent with:** `livekit` (#843), `terminal`, `playground`
**Blocks:** `prstack` (opens the create-session dialog), `session`

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [x] Package skeleton + workflow
- [x] Modules moved (incl. `attachments/`, `baseBranchChoice`, `startProgress`); `tddy-web` imports rewritten
- [x] Specs, page objects, fixtures moved and green from the package config (35 specs, each run alone)
- [x] Boundary rule
- [x] Code-issue record moved

## Technical Changes

### State A (Current)

Create-session lives in `tddy-web/src/components/sessions/` (~30 `CreateSession*`/`createSession*`/`useCreateSession*` files, `CreateSessionPane.tsx` 1412 lines, recorded oversized) plus `attachments/` (705 lines). Its outward imports: generated `session_pb`, `project_pb`, `worktree_pb`, `session_files_pb`; `rpc/useHostFanOut`; `ui/{inline-error,button}`; `lib/{participantRole,testId,projectSelectOptions,branchConflict}`; `hooks/useSessionAttachments`; `sessions/prstack/baseBranchChoice`. `rpc`/`hooks` reach back into it (`CreateSessionProviderField`, `attachments/{HostDocumentPicker,pendingAttachment}`, `startProgress`).

### State B (Target)

`tddy-session-create-web` owns the flow; `tddy-web` (and later `prstack`, `session`) import it. The upward reaches from `rpc`/`hooks` are gone because those modules moved into the package or into `tddy-client-web`.

### Delta

- `git mv` of the modules and specs; import rewrites; aliases.

## Implementation Milestones

- [x] Package specs green
- [x] `tddy-web` builds; PR-stack start-session specs still green

## Testing Plan

**Level:** Cypress component specs from the package config (existing assertions; mount helper swapped to `mountWithClients`), `bun test` for pure helpers that have suites, a boundary rule.

## Acceptance Tests

### tddy-session-create-web

- The ~44 moved specs (`cypress/component/CreateSession*.cy.tsx`, `CreateSessionAttachment*`, `CodebasePlacementRules.cy.ts`, `CreateSessionRequestBuilder.cy.ts`, `createSessionRequestParity.cy.ts`, `stackParentsParity.cy.ts`, `SessionStartAndIndexingProgress.cy.tsx`, `ProjectHomeSetup*`) — unchanged assertions.
- `src/packageBoundary.test.ts` — `tddy-session-create-web imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **`baseBranchChoice` moves here**, not to prstack: create-session consumes it and prstack depends on create-session anyway.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

Scoped, local (full-workspace verification is CI's):

- `tddy-session-create-web`: `bun test src` 87 pass / 0 fail (boundary + exported surface included); all 35 Cypress component specs run one at a time, all passing.
- `tddy-ui-web`: `bun test src` 12 pass. `tddy-client-web`: `bun test src` 65 pass, 4 fail — the presence/media port tests that are `feature/web-split/livekit`'s (#843) red contract, not touched here.
- `tddy-web`: `bun run build` succeeds; bun tests for `fileUploadChunks`, `prStackOrchestrators` and `prstack/` 155 pass; Cypress (one at a time): `CreateSessionAcceptance`, `...AndStreamsVisibleErrors`, `...AutoClosesDrawer`, `...BranchConflict`, `ProjectHomeSetupPaneAcceptance`, `CreateSessionClonePassphrase{Acceptance,Mount}`, `SessionIndexingIndicatorAcceptance`, `PrStackStartSessionModal`, `PrStackStartSessionDocAttachments` all passing. The other four PR-stack start-session specs were not run locally (CI).
- Stack gate: no deletions and no #843-owned file (`ports/media.tsx`, `testkit/media.tsx`, the LiveKit import rule) touched; `#843` is a branch-line predecessor of this node, not a dependency.

## TODO

- [x] Record initial discovery (`2026-11-07-create-session-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (package specs: see Validation Results)
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`tddy-session-create-web` Cypress + boundary rule; `tddy-web` build + PR-stack start-session specs) — verify 100% pass
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
