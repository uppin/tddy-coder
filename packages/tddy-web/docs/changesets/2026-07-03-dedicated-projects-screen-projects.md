# 2026-07-03 — Dedicated Projects screen (`/projects`)

**Type:** Feature

new `ProjectsAppPage`/`ProjectsScreen` (route + `DaemonNavMenu` item) lists projects grouped by logical `projectId` with a row per hosting daemon, hosts an "Add to host" selector sourced from `listEligibleDaemons` (excluding hosts already hosting the project) calling `addProjectToHost`, and takes over the create-project form relocated from `ConnectionScreen`. [projects-screen.md](../projects-screen.md); feature [projects-screen-multi-host.md](../../../../docs/ft/web/projects-screen-multi-host.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web, tddy-service, tddy-daemon)
