# Projects screen (`/projects`)

Dedicated project-management screen in daemon mode, reachable from the `DaemonNavMenu`
(hash route `/projects`, `PROJECTS_ROUTE`/`isProjectsPath` in `routing/appRoutes.ts`,
dispatched in `index.tsx`). Product spec: [projects-screen-multi-host.md](../../../docs/ft/web/projects-screen-multi-host.md).

## Structure

Container + presentational split, mirroring `VmsAppPage`/`WorktreesAppPage`:

- **`components/projects/ProjectsAppPage.tsx`** — data container. `useHttpClient(ConnectionService)`
  polls `listProjects` and wires `createProject` + `addProjectToHost`, refetching after each. The
  selectable **hosts** are the **daemon-role participants** in the common LiveKit room: it joins via
  `useCommonRoom` (mirroring `RpcPlaygroundAppPage`) and derives the host list with
  `daemonHostsFromParticipants` (`lib/participantRole`) — only genuine daemons own projects, so
  coder/session and browser participants are never offered as hosts. Renders `DaemonNavMenu` +
  `ProjectsScreen`.
- **`components/projects/ProjectsScreen.tsx`** — pure props + local UI state. Groups
  `ProjectEntry[]` by logical `projectId` (a project may span hosts), rendering one card per
  project with a **row per hosting `daemonInstanceId`** (and its `mainRepoPath`). Provides:
  - the create-project form (relocated from `ConnectionScreen`), and
  - a per-project **"Add to host"** control: a `<select>` over the daemon hosts (`DaemonHost[]`),
    **excluding hosts that already host that project**; submit calls `addProjectToHost({ projectId,
    name, gitUrl, daemonInstanceId })`. The toggle is disabled when no target hosts remain.

## Role derivation

Both this screen's host list and the presence table's Role column classify a common-room participant
with the shared `lib/participantRole.ts` (`inferParticipantRole`, `metadataLooksLikeDaemonAdvertisement`,
`parseDaemonAdvertisement`, `daemonHostsFromParticipants`). This is the client-side mirror of the
daemon's `eligible_daemon_from_participant_fields`, so client host selection and the server's
`AddProjectToHost` routing agree on what counts as a daemon.

## Test IDs

All `data-testid` values are centralized in `cypress/support/testIds.ts` (static in `TEST_IDS`,
dynamic per-project helpers `projectCard`/`projectHostRow`/`projectAddToHost*`), consumed via the
`projectsScreenPage` page object. Acceptance: `cypress/component/ProjectsScreenAcceptance.cy.tsx`.

The old sessions view (`ConnectionScreen`) keeps its per-project **session** accordions; only the
create-project form moved here.
