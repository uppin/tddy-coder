# Session inspector, agents and code pane in `tddy-session-web` - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 17/20 — branch `feature/web-split/session-inspector`

## Affected Features

- **Primary**: [session-files-inspector.md](../session-files-inspector.md), [session-usage-inspector.md](../session-usage-inspector.md), [session-code-pane.md](../session-code-pane.md).

## Summary

Move the session inspector and its tabs (details, activity, files, tools, usage, worktree, screen sharing, VNC), `InspectorTabs`, the agent roster/tree/conversation layers, `WorktreeCodePane` and `components/session/*` (to `src/code/`) into `tddy-session-web`.

## Background

Split out of `feature/web-split/session` (#851) on 2026-11-07. That node's green stopped on its boundary
warning (~19k lines, 150 files, 156 specs, ~40 unplanned reaches into tddy-web). The developer chose four
capability nodes: seams + helpers (#851), runtime + terminals, inspector + agents + code pane, drawer +
project home.

## Proposed Changes

### What's Changing

- Those modules and ~40 specs (`SessionInspector*`, `SessionAgent*`, `CodeHover*`, `CodeIndex*`, `RestructurePlan*`, `WorktreeCodePane*`, agent-roster specs) move into the package.

### What's Staying the Same

- Every session behaviour. Specs keep their assertions and mount through the port doubles; specs that
  imported LiveKit testkits switch to `aPresenceDouble` / `aRoomRpcDouble` / `aMediaDouble`.

## Acceptance Criteria

- [ ] Inspector/agent/code-pane modules live in `tddy-session-web`; no tddy-web or LiveKit import.
- [ ] Their specs pass from the package.

## References

- Predecessor: `feature/web-split/session-runtime`. Package: `tddy-session-web` (created by #851).
