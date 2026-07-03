# Projects screen (`/projects`)

Dedicated project-management screen in daemon mode, reachable from the `DaemonNavMenu`
(hash route `/projects`, `PROJECTS_ROUTE`/`isProjectsPath` in `routing/appRoutes.ts`,
dispatched in `index.tsx`). Product spec: [projects-screen-multi-host.md](../../../docs/ft/web/projects-screen-multi-host.md).

## Structure

Container + presentational split, mirroring `VmsAppPage`/`WorktreesAppPage`:

- **`components/projects/ProjectsAppPage.tsx`** — data container. `useHttpClient(ConnectionService)`;
  polls `listProjects` + `listEligibleDaemons`; wires `createProject` and `addProjectToHost`,
  refetching after each. Renders `DaemonNavMenu` + `ProjectsScreen`.
- **`components/projects/ProjectsScreen.tsx`** — pure props + local UI state. Groups
  `ProjectEntry[]` by logical `projectId` (a project may span hosts), rendering one card per
  project with a **row per hosting `daemonInstanceId`** (and its `mainRepoPath`). Provides:
  - the create-project form (relocated from `ConnectionScreen`), and
  - a per-project **"Add to host"** control: a `<select>` populated from `listEligibleDaemons`,
    **excluding hosts that already host that project**; submit calls `addProjectToHost({ projectId,
    name, gitUrl, daemonInstanceId })`. The toggle is disabled when no target hosts remain.

## Test IDs

All `data-testid` values are centralized in `cypress/support/testIds.ts` (static in `TEST_IDS`,
dynamic per-project helpers `projectCard`/`projectHostRow`/`projectAddToHost*`), consumed via the
`projectsScreenPage` page object. Acceptance: `cypress/component/ProjectsScreenAcceptance.cy.tsx`.

The old sessions view (`ConnectionScreen`) keeps its per-project **session** accordions; only the
create-project form moved here.
