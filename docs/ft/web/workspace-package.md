# Workspace package (`tddy-workspace-web`)

**Package:** `packages/tddy-workspace-web`
**Applies to:** the Accounts, Projects, Settings, Models & Agents, Tasks and Worktrees screens, the
session's worktree tab, and the sign-in dialogs
**Added:** 2026-11-07 (#web-split 13/17, PR #849)

## Overview

The workspace screens — each one daemon service's list or form — and the sign-in dialogs are one
package. It reaches daemons only through `tddy-client-web` and renders with `tddy-ui-web`; it does not
depend on the web app or on LiveKit. The page shells, routing and the sign-in provider stay in the web
app.

## Behaviour

- Every screen behaves as before ([accounts-screen.md](accounts-screen.md),
  [projects-screen-multi-host.md](projects-screen-multi-host.md), [models-and-agents.md](models-and-agents.md),
  [tasks-ui-realtime.md](tasks-ui-realtime.md), [worktrees.md](worktrees.md),
  [codex-oauth-web-relay.md](codex-oauth-web-relay.md)).

## Acceptance criteria

- [x] The screens live in `tddy-workspace-web` and import nothing from `tddy-web` or LiveKit (rule test).
- [x] Their specs pass from the package (`web.yml` runs them).
- [x] The app's routes, shell navigation and sign-in flows work as before (`tddy-web`'s wiring specs pass).

Technical reference: [packages/tddy-workspace-web/docs/README.md](../../../packages/tddy-workspace-web/docs/README.md).
