# 2026-11-07 — The create-session flow is its own package (#web-split 7/17, PR #844)

- New package: `CreateSessionPane`, `CreateSessionDialog`, `ProjectHomeSetupPane`, every `CreateSession*`
  field and section, their hooks (`useCreateSession*`, `useAvailableAgents`, `useProjectBranches`,
  `useSessionAttachments`, `useAgentModels`, `useProviderRoutes`, `useSelectableAgents`, …), the request
  builder (`buildStartSessionRequest`, `buildProjectHomeStartSpec`), codebase placement, start progress,
  `attachments/`, `BranchConflictDialog`, `AgentKindBadge`, and the `lib/` helpers only it used — all
  `git mv`'d from `tddy-web`.
- `BaseBranchOption` / `BaseBranchChoice` live here (`baseBranchOptions.ts`); the `baseBranchChoice()`
  resolver stays with the PR-stack screen.
- `StartPassphrasePrompt` is a slot: `StartPassphraseProvider` supplies the component, and the prompt
  throws without one (`StartPassphrasePrompt.test.tsx`).
- `CreateSessionSshConfigSelect` is reachable when the host connection reports `connected`; it no longer
  reads the test-only `useLiveKitTransportFactoryIsOverridden`.
- 35 Cypress specs run from the package's config with `cy.mountWithClients` and per-host in-memory
  backends (`cypress/support/mounts.tsx`); `src/packageBoundary.test.ts` pins the import rule and the
  exported surface; `.github/workflows/web-tddy-session-create-web.yml`.
- The `CreateSessionPane.tsx` oversized-file record moved with the file (581 lines, unchanged).
- Deferred: [support copies](../../../../docs/dev/todo/2026-11-07-create-session-cypress-support-duplicates-tddy-web.md),
  [typecheck errors](../../../../docs/dev/todo/2026-11-07-create-session-package-typecheck-errors.md).
