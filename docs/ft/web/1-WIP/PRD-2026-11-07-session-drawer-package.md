# Sessions drawer and project home in `tddy-session-web` - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 18/20 — branch `feature/web-split/session-drawer`

## Affected Features

- **Primary**: [session-drawer.md](../session-drawer.md), project home ([web-workspace-setup.md](../web-workspace-setup.md)).

## Summary

Move the sessions drawer and project home into `tddy-session-web`: `SessionsDrawerScreen`, `SessionDrawer*`, `ProjectHomeEntry`, `useSessionsDrawer*`, `useEnsureProjectHome`, `useProjectHomeSetup`, `NewTerminalMenu`, `AgentPicker`, `StatusBar`, `SessionTrafficStrip` — and the ~60 drawer-driven specs other nodes left behind (PR-stack, create-session, host, provider-defaults).

## Background

Split out of `feature/web-split/session` (#851) on 2026-11-07. That node's green stopped on its boundary
warning (~19k lines, 150 files, 156 specs, ~40 unplanned reaches into tddy-web). The developer chose four
capability nodes: seams + helpers (#851), runtime + terminals, inspector + agents + code pane, drawer +
project home.

## Proposed Changes

### What's Changing

- Those modules and ~60 drawer-driven specs move into the package; tddy-web keeps only the route that renders the drawer.

### What's Staying the Same

- Every session behaviour. Specs keep their assertions and mount through the port doubles; specs that
  imported LiveKit testkits switch to `aPresenceDouble` / `aRoomRpcDouble` / `aMediaDouble`.

## Acceptance Criteria

- [ ] Drawer and project-home modules live in `tddy-session-web`; no tddy-web or LiveKit import.
- [ ] The drawer-driven specs pass from the package.
- [ ] tddy-web holds no session component.

## References

- Predecessor: `feature/web-split/session-inspector`. Package: `tddy-session-web` (created by #851).
