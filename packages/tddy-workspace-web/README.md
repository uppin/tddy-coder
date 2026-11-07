# tddy-workspace-web

The workspace screens: accounts, projects, daemon settings, models and agents, tasks, worktrees (with
the session worktree tab), and the sign-in dialogs (`CodexOAuthDialog`, `DeviceLoginPanel`,
`CredentialVaultPrompt`, `DaemonLoginScreen`, `GitHubLoginButton`, `UserAvatar`).

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients`).

Documentation: [docs/README.md](docs/README.md).
