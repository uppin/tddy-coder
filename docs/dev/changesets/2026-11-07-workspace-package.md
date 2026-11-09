# 2026-11-07 — `tddy-workspace-web`: the workspace screens in one package (#web-split 13/17, PR #849)

**Type:** Architecture change

Adds `packages/tddy-workspace-web` (workspace entry, `bun.lock`) with the accounts, projects, daemon
settings, models and agents, tasks and worktrees screens and the sign-in dialogs — 33 Cypress specs,
their suites and the screen docs with them. `tddy-web` keeps the `*AppPage` shells, `SettingsAppPage`,
`AuthCallback`, the `AuthProvider` and seven wiring specs. Shared pieces were lowered rather than
copied: session route builders, `AuthFlowDeclaration` and `useActionFailure` to `tddy-client-web`,
`standaloneFormStyles` to `tddy-ui-web`. The package joins `.github/workflows/web.yml` by workspace
discovery (caller workflow deleted) and pre-bundles through `scripts/vite-prebundle.ts`; `Web lint`
excludes it until its moved Cypress fixtures typecheck.
Deviations from the plan: 33 specs moved, not ~40 — seven exercise the real `AuthProvider`, `AppShell`
or `index.tsx` and stay in `tddy-web`; `lib/codexOauthMetadata` did not exist; `SettingsAppPage`
stayed (six lines of wiring).
Measured: `useModelRegistryFanOut.ts` 613 → 612 lines (record open, moved with the file).
Package detail: [tddy-workspace-web](../../packages/tddy-workspace-web/docs/changesets/2026-11-07-workspace-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-workspace-package.md),
[tddy-client-web](../../packages/tddy-client-web/docs/changesets/2026-11-07-workspace-package.md),
[tddy-ui-web](../../packages/tddy-ui-web/docs/changesets/2026-11-07-workspace-package.md).
Product description: [workspace-package.md](../ft/web/workspace-package.md).
Deferred: [moved Cypress fixtures do not typecheck](../todo/2026-11-07-workspace-web-specs-do-not-typecheck.md).
