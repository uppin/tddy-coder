# Session watches are not shown on the web

**Recorded:** 2026-11-16, by `#activity-wake` 2/3 (PR #930).

## What is missing

An orchestrator's session watches (`CreateSessionWatch` / `ListSessionWatches` /
`RenewSessionWatch` / `CancelSessionWatch`, with matched events riding on its tool answers as
`watchEvents`) have no web surface: no RPC lists them, and no view shows an orchestrator's live
watches, their filters, expiries or waiting counts. The watches themselves are the daemon's
(`tddy_session_activity::session_watches`, persisted per project); only their display is absent.

## Developer decision

2026-11-16: the developer chose not to build a web view in the `#activity-wake` stack. This is a
future enhancement, not a gap in the stack: delivery on tool answers (and the successor's wake)
is the whole of this stack's surface.

## What would close it

A read of the daemon's watch store for a project (live watches with their waiting counts, in the
shape `ListSessionWatches` already answers), an RPC carrying it, and a view that shows it beside
the orchestrator's sessions — plus the usual Storybook/Cypress coverage for the new surface.

## Where

- Store: `packages/tddy-session-activity/src/session_watches.rs`
- Tools: `packages/tddy-tool-engine/src/project_home_watch_tools.rs`
- PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-16-session-watches.md`
