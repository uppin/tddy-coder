# Projects screen (`/projects`)

Dedicated project-management screen in daemon mode, reachable from the `DaemonNavMenu`
(hash route `/projects`, `PROJECTS_ROUTE`/`isProjectsPath` in `routing/appRoutes.ts`,
dispatched in `index.tsx`). Product spec: [projects-screen-multi-host.md](../../../docs/ft/web/projects-screen-multi-host.md).

## Structure

Container + presentational split, mirroring `VmsAppPage`/`WorktreesAppPage`:

- **`components/projects/ProjectsAppPage.tsx`** — data container. `useDaemonClient(ConnectionService)`
  (shared `SelectedDaemonProvider`) polls `listProjects` and wires `createProject`; the selectable
  **hosts** come from the same shared context (`useDaemons()`) — daemon-role participants in the
  common LiveKit room, classified by `daemonHostsFromParticipants` (`lib/participantRole`), so
  coder/session and browser participants are never offered as hosts. **`addProjectToHost` addresses
  the chosen target host directly**: it builds a client for `daemon-{targetInstanceId}` from the
  shared room + `useLiveKitTransportFactory` (rather than routing through the selected daemon and
  relying on a peer-forward hop), sending `userRelativePath` through to the RPC. List/create still
  use the selected-daemon client. Renders `DaemonNavMenu` + `ProjectsScreen`.
- **`components/projects/ProjectsScreen.tsx`** — pure props + local UI state. Groups
  `ProjectEntry[]` by logical `projectId` (a project may span hosts), rendering one card per
  project with a **row per hosting `daemonInstanceId`** (and its `mainRepoPath`). Provides:
  - the create-project form (relocated from `ConnectionScreen`), and
  - a per-project **"Add to host"** control: a `<select>` over the daemon hosts (`DaemonHost[]`),
    **excluding hosts that already host that project**, plus an **optional clone-location input**
    (path relative to the target host's home → `userRelativePath`; blank ⇒ the host's default
    `repos_base_path`). Submit calls `addProjectToHost({ projectId, name, gitUrl, daemonInstanceId,
    userRelativePath })`. The toggle is disabled when no target hosts remain. Each host also shows
    its advertised **base clone location** (`DaemonHost.reposBasePath`, from the daemon's
    common-room advertisement).

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
