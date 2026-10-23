# 2026-10-23 — Pinned "main" with no home agent links to the project's home-agent picker

**Type:** Fix

Cross-package entry: [2026-10-23-project-home-setup-link.md](../../../../docs/dev/changesets/2026-10-23-project-home-setup-link.md).

- **`ProjectHomeEntry`** takes `onSetUpProjectHome?(projectId)` and calls it, instead of
  `onEnsure`, when the project has no home agent. The entry is `aria-disabled` only when that
  handler is absent; the hint stays. `SessionDrawer` passes the prop through.
- **`routing/projectSetupLocation.ts`**: `projectSetupLocation(location, projectId)` returns
  `#/projects?project=<id>`, keeping `host` and dropping the Sessions screen's params.
  `useSessionsDrawerInspector` exposes `openProjectSetup`, which pushes it, and
  `SessionsDrawerScreen` wires it to the drawer.
- **Projects focus from the address:** `ProjectsAppPage` reads `PARAM_PROJECT` into
  `ProjectsScreen.focusedProjectId` → `ProjectCard.focusHomeAgent` → `ProjectHomeAgent.focused`,
  which focuses and scrolls the `<select>` once per arrival.
- **Tests:** `cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx` (4) on the shared
  `projectsAppPageFixtures`, `src/routing/projectSetupLocation.test.ts` (3).
- **Code issue opened:** `oversized-file-sessions-drawer-screen`, 500 lines (`wc -l`), 498 at base
  `00abd1554`. Split deferred until the `#visible-failures` stack lands, with the developer's consent.

See [projects-screen.md § Home agent](../projects-screen.md#home-agent).
