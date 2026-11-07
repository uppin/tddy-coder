# 2026-11-07 — Workspace screens moved to `tddy-workspace-web` (#web-split 13/17, PR #849)

`AccountsAppPage`, `ProjectsAppPage`, `ModelsAppPage`, `WorktreesAppPage` and `TasksDrawerScreen` are
shells around the package's `*Content` / `TasksScreen`; `SettingsAppPage`, `AuthCallback` and the
`AuthProvider` stay. `appRoutes` re-exports the session route builders now in `tddy-client-web`.
Seven wiring specs stay here and import the sign-in components from the package:
`AuthProviderRefreshAcceptance`, `StandaloneFormThemeAcceptance`, `RedirectLoginAcceptance`,
`DeviceLoginAcceptance`, `CredentialVaultPromptAcceptance`, `DaemonSettingsRouteAcceptance`,
`models/ModelsNavAcceptance`. The screen docs moved to the package; `daemon-sign-in.md` stays (it
documents `useAuth`) with the component paths updated. `AccountsScreen.stories.tsx` stays (Storybook
globs this package).
