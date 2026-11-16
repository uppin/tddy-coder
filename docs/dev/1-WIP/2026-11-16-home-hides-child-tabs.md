# Home and orchestrator sessions show no child-conversation tabs

**Type:** Fix
**Packages:** `tddy-session-web`

## Problem

A project-`home` session (and any `session_role: orchestrator` session) shows a long row of
identical "conversation" tabs, one for every session it ever started. Clicking one flickers and
sends the operator straight back to the Agent tab.

`useChildSessions` turns every `SessionEntry` with `orchestratorSessionId === parentSessionId`
into a child tab. Since `#start-parity`, every session a home/orchestrator starts through
`StartSession` records that session as orchestrator — each becomes a tab, labelled "conversation"
(it has no `recipe`). Most of those sessions have ended, so selecting one attaches, the stream
ends, `dropChild` clears the selection, and the Agent pane comes back.

Child tabs were designed for the grill-me `spawn_conversation` hand-off
(`docs/ft/coder/spawn-conversation.md`), which must keep working.

## Change

`useChildSessions` (`packages/tddy-session-web/src/useChildSessions.ts`) returns no children when
the parent entry carries `sessionRole` `project-home` or `orchestrator` — the existing
`PROJECT_HOME_ROLE` / `ORCHESTRATOR_ROLE` constants in `utils/projectHomeGroups.ts`, the same
fields the drawer already uses to mark those sessions. No proto field and no daemon change. With
no child sessions, `SessionTerminalTabs` renders no child tabs and `SessionRuntime` mounts no
child panes (a tab that cannot be selected can never attach). An ordinary (non-home,
non-orchestrator) session that spawned a child still shows the tab.

## Tests

New Cypress component spec `SessionHomeHidesChildTabs.cy.tsx`, next to
`SessionChildTabsAcceptance.cy.tsx`, written failing first: a project-home session with started
sessions shows only the Agent tab; an orchestrator session with started sessions shows only the
Agent tab; an ordinary session with a spawned child still shows the child tab. The existing
acceptance spec is the standing regression guard for the grill-me flow.

Scoped run from `packages/tddy-session-web` (Xvfb supplied from nix, since Cypress requires it
and the dev shell does not ship it):

```bash
bun run cypress:component --spec cypress/component/SessionHomeHidesChildTabs.cy.tsx,cypress/component/SessionChildTabsAcceptance.cy.tsx
```

→ 6 passing (3 new + 3 existing), 0 failing.

## Documentation

`docs/ft/coder/spawn-conversation.md` gains the exclusion where it states the child-tab derivation
contract. `docs/ft/web/session-terminal-tabs.md` needs no change: its statements about child tabs
(the `+` menu's tab kinds, the full-screen active pane) stay true for the sessions that have them.
