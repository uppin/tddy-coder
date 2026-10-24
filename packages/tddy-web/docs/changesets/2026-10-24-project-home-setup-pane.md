# 2026-10-24 — Pinned "main" with no home agent opens a setup pane on the Sessions screen

**Type:** Feature

PR [#712](https://github.com/uppin/tddy-coder/pull/712).

- **Route:** `SESSIONS_PROJECT_HOME_SETUP_ROUTE` (`/sessions/project-home`),
  `sessionsProjectHomeSetupPath(projectId)` and `parseSessionsProjectHomeSetupProjectId(path)` in
  `routing/appRoutes.ts`; `isSessionsDrawerPath` accepts `/sessions/project-home/:projectId`, which is
  never read as a session id.
- **`ProjectHomeSetupPane`** (`components/sessions/`): *Set up main for &lt;project&gt;*, the shared
  `ProjectHomeAgent` picker with `focused`, **Start main session** (disabled until the project row
  carries a `homeAgent`) and **Cancel**. A start refusal is shown in the pane; a project id the
  registry does not list renders a not-listed message and no picker.
- **`useProjectHomeSetup`**: the open project from the path, `open` (push), `close` (→ `/sessions`)
  and `setHomeAgent`, which calls `SetProjectHomeAgent` on the selected host and then `onStored`.
- **`SessionsDrawerScreen`** renders the pane in place of `SessionMainPane` while the path names a
  project; Start goes through `useEnsureProjectHome`. On mobile the entry's click closes the list.
- **`ProjectHomeEntry`** shows no hint text. `useSessionsDrawerProjects` returns `reload`;
  `useSessionsDrawerProjectHomes` returns `{ homes, agents }`.
- **`useSessionsDrawerClients`**: the screen's eleven per-service `useDaemonClient` calls, moved
  verbatim.
- **Removed:** `routing/projectSetupLocation.ts` and its test; `useSessionsDrawerInspector` no longer
  returns `openProjectSetup`.
- **Tests:** `cypress/component/ProjectHomeSetupPaneAcceptance.cy.tsx` (7);
  `src/routing/appRoutes.test.ts` (+4); `SessionsDrawerProjectHomeAcceptance` asserts no hint.
- **Code issue closed:** `oversized-file-sessions-drawer-screen` — final measurement **490 lines**
  (`wc -l packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx`), budget 500, 0.98×.
  History: 511 → 498 (#706) → 500 (#707) → 490 (this change: −33 for the clients hook, +23 for the
  pane wiring). Record deleted.
- **Backlog entry resolved:** *SessionsDrawerScreen.tsx reached the 500-line budget under
  `#visible-failures` 2/2* (`2026-10-23-sessions-drawer-screen-at-500-lines`) — the screen is under
  budget, the clients hook is extracted and `openProjectSetup` is gone. Its optional second seam
  (selection/create handlers into a `useSessionsDrawerSelection` hook) was not taken. Entry deleted,
  with the developer's agreement.

See [projects-screen.md § Home agent](../projects-screen.md#home-agent) and
[session drawer § Pinned main entry](../../../../docs/ft/web/session-drawer.md#pinned-main-entry).
