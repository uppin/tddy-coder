# Changeset: project-home-setup-pane

PRD (target): [`docs/ft/web/session-drawer.md` § Pinned main entry](../../ft/web/session-drawer.md#pinned-main-entry)
(updated in place, with [`url-state-routing.md`](../../ft/web/url-state-routing.md) and
[`projects-screen-multi-host.md` § Home agent](../../ft/web/projects-screen-multi-host.md#home-agent))

## Problem

After [#707](https://github.com/uppin/tddy-coder/pull/707), a project with no home agent showed a
grey hint under its pinned **main** entry — *"Set a home agent for this project on the Projects
screen to start its main session."* — and a click on the entry left the Sessions screen for
`#/projects?project=<id>`. Setting up the main session meant leaving the screen it runs on.

## State B

- The hint is gone. A click on **main** with no home agent opens a **setup pane** in the Sessions
  detail area at `#/sessions/project-home/<projectId>` (push): the project's **Home agent** picker
  (`ProjectHomeAgent`, focused), **Start main session** (enabled once a home agent is stored) and
  **Cancel** (→ `#/sessions`).
- Choosing an agent calls `SetProjectHomeAgent` on the selected host, then re-reads `ListProjects`
  (`useSessionsDrawerProjects().reload`), so the entry shows the label and Start enables.
- Start calls `EnsureProjectHomeSession` through the existing `useEnsureProjectHome`, whose selection
  replaces the pane. Refusals of either call are shown in the pane.
- The Projects screen's `?project=<id>` focus stays as a deep link; nothing in the app links to it
  any more.
- No daemon or RPC changes.

## Code

| File | Change |
|---|---|
| `src/routing/appRoutes.ts` | `SESSIONS_PROJECT_HOME_SETUP_ROUTE`, `sessionsProjectHomeSetupPath`, `parseSessionsProjectHomeSetupProjectId`; `isSessionsDrawerPath` accepts the two-segment path |
| `src/components/sessions/ProjectHomeSetupPane.tsx` | new — the pane |
| `src/components/sessions/useProjectHomeSetup.ts` | new — URL-derived open project, open/close, `SetProjectHomeAgent` |
| `src/components/sessions/useSessionsDrawerClients.ts` | new — the screen's eleven `useDaemonClient` calls, moved verbatim (the first seam of `oversized-file-sessions-drawer-screen`) |
| `src/components/sessions/SessionsDrawerScreen.tsx` | renders the pane in place of `SessionMainPane`; 500 → 490 lines |
| `src/components/sessions/ProjectHomeEntry.tsx` | hint removed |
| `src/components/sessions/useSessionsDrawerInspector.ts` | `openProjectSetup` removed |
| `src/components/sessions/useSessionsDrawerProjectHomes.ts` | returns `{ homes, agents }` |
| `src/components/sessions/useSessionsDrawerProjects.ts` | adds `reload` |

TODO: `src/routing/projectSetupLocation.ts` and its test are now unused — delete with the
developer's consent.

## Package docs to update at wrap (`packages/tddy-web/docs/`)

- `projects-screen.md` § Home agent, *Focus from the address*: drop the paragraph on the drawer
  building `projectSetupLocation` and pushing it from `useSessionsDrawerInspector().openProjectSetup`;
  say the drawer opens `ProjectHomeSetupPane` instead, which reuses `ProjectHomeAgent` with
  `focused`. Drop `projectSetupLocation.test.ts` from its test list once deleted.
- `code-issues/oversized-file-sessions-drawer-screen.md`: updated in place in this change (490 lines,
  first seam taken, `openProjectSetup` gone).

## Tests

- `cypress/component/ProjectHomeSetupPaneAcceptance.cy.tsx` — new, 7 (screen-level over the in-memory
  daemon: open, no hint, store, start, cancel; pane-level: start refusal, unknown project).
- `cypress/component/SessionsDrawerProjectHomeAcceptance.cy.tsx` — the hint test now asserts no hint.
- `cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx` — header only.
- `src/routing/appRoutes.test.ts` — 4 for the new route.
