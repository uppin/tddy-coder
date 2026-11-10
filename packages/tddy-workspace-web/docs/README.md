# tddy-workspace-web documentation

The workspace screens — one daemon service's list or form each — and the sign-in dialogs:

| Area | Source | Reference |
|---|---|---|
| Accounts | `src/accounts/` (`AccountsContent`, `AccountsScreen`) | [accounts-screen.md](accounts-screen.md) |
| Projects | `src/projects/` (`ProjectsContent`, `ProjectsScreen`, `ProjectCard`, `ProjectLocalGh`, `ProjectMainSessionLink`) | [projects-screen.md](projects-screen.md) |
| Daemon settings | `src/settings/` (`DaemonSettingsScreen`, `settingsForm`, `integrationStatusText`) | [daemon-settings-screen.md](daemon-settings-screen.md) |
| Models and agents | `src/models/` (`ModelsContent`, `ModelsScreen`, panels, assistant dialogs, `ModelChatDialog`, `useModelRegistryFanOut`) | [models-screen.md](models-screen.md) |
| Tasks | `src/tasks/` (`TasksScreen`, `TaskOutputPane`, the task streams, `tasksRoutes`) | [docs/ft/web/tasks-ui-realtime.md](../../../docs/ft/web/tasks-ui-realtime.md) |
| Worktrees | `src/worktrees/` (`WorktreesContent`, `WorktreesScreen`, `SessionWorktreeTab`, `useWorktreeStatsStream`) | [docs/ft/web/worktrees.md](../../../docs/ft/web/worktrees.md) |
| Sign-in | `src/signin/` (`DaemonLoginScreen`, `DeviceLoginPanel`, `CredentialVaultPrompt`, `GitHubLoginButton`, `UserAvatar`, `CodexOAuthDialog`) | [codex-oauth-dialog.md](codex-oauth-dialog.md), [tddy-web daemon-sign-in.md](../../tddy-web/docs/daemon-sign-in.md) |

Each `*Content` is the data half of what was a `tddy-web` `*AppPage`; the pages, `SettingsAppPage`,
`TasksDrawerScreen`'s shell, `AuthCallback` and the `AuthProvider` behind `AuthPort` stay in
`tddy-web` as wiring. The package imports nothing from `tddy-web` and nothing from LiveKit
(`src/packageBoundary.test.ts`). Diagnostics: `tddyDebug` namespace `tddy:workspace`.

- **Changeset history**: [changesets/](changesets/)
- **Open code issues**: [code-issues/](code-issues/)

## Tests

`bun test src` (settings form, integration status, model registry merge and chat target, tasks routes,
worktree helpers, the boundary rule) and `bun run cypress:component` — 33 component specs from the
package's own `cypress.config.ts`, mounted with `cy.mountWithClients` and port doubles (models through
`cypress/support/mountModels.tsx`). Seven specs whose subject is `tddy-web` wiring (the real
`AuthProvider`, `AppShell`, `index.tsx`) stay there. CI: `.github/workflows/web.yml`; `Web lint`
excludes the package until its moved Cypress fixtures typecheck (`scripts/web-lint-exclusions.json`).
