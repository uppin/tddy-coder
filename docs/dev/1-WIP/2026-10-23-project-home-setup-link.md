# 2026-10-23 — Clicking "main" with no home agent takes you to the project's home-agent picker

**Type:** Bug fix · **Stack:** `#visible-failures` 2/2, on #706 · **Status:** 🚧 Draft (reproduction
tests red)

## Summary

A project's pinned **main** entry in the sessions drawer, from `#project-home` 4/7, renders
`aria-disabled` when the project has no home agent, and its click returns early. The only
explanation is a small grey hint under the entry. Clicking "main" therefore produces nothing: no
request, no navigation, no error. Reported from a live `./web-dev`, where all three projects had no
home agent, and confirmed with a headless browser: no `EnsureProjectHomeSession` was sent, and the
daemon never received a `SetProjectHomeAgent`.

After this change, clicking "main" on such a project takes the operator to **Projects** with that
project's home-agent picker focused. A project that has a home agent still starts its main session.

## Responsibility

- `SessionDrawer` / `ProjectHomeEntry`: with no home agent, a click calls a new
  `onSetUpProjectHome(projectId)`, and the entry is no longer `aria-disabled`. The hint stays.
- `SessionsDrawerScreen` wires `onSetUpProjectHome` to navigation via
  `projectSetupLocation(current, projectId)` (`src/routing/projectSetupLocation.ts`). That helper
  returns `#/projects?project=<id>`, keeps the selected host and drops the Sessions screen's own
  selection params.
- The Projects screen reads `PARAM_PROJECT`, then focuses that project's home-agent `<select>` and
  scrolls it into view.

## Boundaries

- Does not change what happens when a project **has** a home agent. That path ensures and selects
  the home session, exactly as `#project-home` 4/7 defined it.
- Does not change how a home agent is stored (`SetProjectHomeAgent`) or listed.
- No daemon or RPC changes.

## Dependencies

- #706 (`fix/web-dev-livekit-and-visible-errors`) is the base. It makes the drawer and Projects
  reachable under the React Compiler, and it splits `SessionDrawer.tsx` (into `SessionDrawerGroups.tsx`)
  and `ProjectsScreen.tsx` (into `ProjectCard.tsx`). This node edits the post-split files.
- Uses `#project-home`'s `ProjectHomeEntry`, `ProjectHomeAgent` and `projectHomeAgentSelect`, which
  are on `master` and in #706.

## Draft PR contract

What this draft publishes:

- **Reproduction tests (red):**
  - `cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx`:
    - clicking a no-agent "main" calls `onSetUpProjectHome(projectId)` and never ensures;
    - the entry is not `aria-disabled`;
    - `#/projects?project=<id>` focuses that project's home-agent picker;
    - a project with a home agent still ensures (passes today, kept as a guard).
  - `src/routing/projectSetupLocation.test.ts`: the location helper.
- **The contract:**
  - `SessionDrawer` takes `onSetUpProjectHome?: (projectId: string) => void`;
  - `projectSetupLocation(location: AppLocation, projectId: string): AppLocation`;
  - Projects honours `PARAM_PROJECT`.

The fix lands in this same PR before it leaves draft.

## Prerequisites

Backlog and code issues cross-checked for `ProjectHomeEntry`, `SessionDrawer` and the Projects
screen:
- `docs/dev/todo/2026-10-23-project-home-*` covers the running heuristic, git push, auto-stop of
  untracked processes and the overlay mount. None is about this feedback gap. ℹ Not affected.
- [2026-10-23-session-drawer-grew-past-500-by-project-groups.md](../todo/2026-10-23-session-drawer-grew-past-500-by-project-groups.md):
  its split already happened in #706. Deleting the entry is the developer's call there, not this
  node's.

## Scope

- [x] Reproduction tests (red; above)
- [ ] `ProjectHomeEntry` / `SessionDrawer`: `onSetUpProjectHome`, no `aria-disabled`
- [ ] `projectSetupLocation` + `SessionsDrawerScreen` wiring
- [ ] Projects: focus the picker named by `PARAM_PROJECT`
- [ ] The existing `SessionsDrawerProjectHomeAcceptance` specs stay green ("shows a hint instead of
      starting" keeps its hint and no-ensure assertions)
