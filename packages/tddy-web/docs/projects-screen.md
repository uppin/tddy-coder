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

## Account assignment

Each project card carries a per-provider account control, rendered by `ProjectsScreen` from
`ProjectEntry.accounts` and the accounts the vault holds (`AssignableAccount`):

- **One row per provider** the vault has an account at, plus a row for any provider the project is
  assigned at but the vault lacks. A locked, uninitialized or absent vault yields no assignable
  accounts, so it yields no rows for providers the project is not assigned at.
- A row shows one of three distinct states: **assigned** (the account's label selected),
  **no account assigned** (the select's empty-valued option, a real choice and not a placeholder),
  or **unavailable on this host** (the project names an account this host's vault does not hold,
  shown beside the select, never as unassigned).
- The picker's options come from `AccountsService.ListAccounts` (`useAssignableAccounts` in
  `ProjectsAppPage`). Choosing sends `SetProjectAccounts` with the **full** assignment list for the
  project, the other providers' entries unchanged, and the daemon replaces the whole set.

Account assignment is rendered by `ProjectAccountAssignment.tsx` (`assignmentRowsFor`,
`withAssignment`), split out of `ProjectsScreen.tsx`.

Test IDs: `project-account-row-<projectId>-<provider>`, `project-account-select-<projectId>-<provider>`,
`project-account-unavailable-<projectId>-<provider>`. Acceptance:
`cypress/component/ProjectAccountsAcceptance.cy.tsx`.

## Local gh

`components/projects/ProjectLocalGh.tsx` holds the card's **Local gh** controls:

- **`LocalGhToggle`** — a checkbox on the card. Turning it on sends `SetProjectAccounts` with the
  project's assignments minus the GitHub one and `localGh: true`; turning it off sends `localGh:
  false` and leaves no GitHub account assigned. `onSetProjectAccounts`' input carries `localGh`.
- While it is on, the GitHub account row is **hidden** (the picker is not offered); other providers'
  rows stay.
- **`HostGithubState`** — one line per host row, shown while the toggle is on. It calls the
  `loadHostGithubCli({ daemonInstanceId, githubHostname })` prop (`ProjectsAppPage` wires it to
  `HostService.getHostTooling` on that host) for the project's GitHub host, which
  `githubHostnameOf(gitUrl)` reads from the git URL (https, `ssh://`, scp-like; `null` when none, and
  the line then says so instead of asking about a guessed host). It shows *checking*, *logged in to H
  as login*, *not logged in to H*, *gh: not installed*, *could not be read (reason)*, or *could not be
  read on this host*.

Test IDs: `project-local-gh-toggle-<projectId>`, `project-host-gh-<projectId>-<daemonInstanceId>`;
page-object helpers `localGhToggle`, `turnLocalGhOn`, `hostGhState`. Acceptance:
`cypress/component/ProjectLocalGh.cy.tsx`. The card is the first production caller of
`GetHostTooling`.

## Home agent

`components/projects/ProjectHomeAgent.tsx` is the card's **Home agent** `<select>`: `Not set` plus one
option per `agents` entry. `ProjectsAppPage` loads the agents with `CatalogService.ListAgents` on the
selected daemon (`useListedAgents`; a failed load leaves the list empty, as the account list does)
and wires `onSetHomeAgent` to `SetProjectHomeAgent` on `group.hosts[0]`, then reloads the projects.
Unlike the other setters, that callback **rejects** with the daemon's error, which the picker shows
inline (`ConnectError.rawMessage`) while keeping the stored value selected. A stored id missing from
`agents` gets its own *(not listed by this daemon)* option. Both `agents` and `onSetHomeAgent` are
optional `ProjectsScreen` props; without `onSetHomeAgent` the picker is not rendered.

Test IDs: `project-home-agent-select-<projectId>`, `project-home-agent-error-<projectId>`; page-object
helpers `homeAgentValue`, `homeAgentOptionLabels`, `setHomeAgent`, `homeAgentError`. Acceptance:
`cypress/component/ProjectsScreenHomeAgentAcceptance.cy.tsx`.

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
