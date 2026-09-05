# 2026-04-11 — **Connection screen

**Type:** Feature

multi-daemon project rows** — **`ConnectionScreen`**: one accordion + session table per **`ListProjects`** row; composite **`data-testid`** **`projectId__daemonInstanceId`** when **`daemon_instance_id`** is set; **`sessionProjectTable`** host-scoped helpers (**`connectionProjectRowKey`**, **`sessionBelongsToProjectHost`**, **`sortedSessionsForProjectHostTable`**, **`isSessionOrphan`**); Cypress **`ConnectionScreen.cy.tsx`**, Bun **`sessionProjectTableMultiHost.test.ts`**. Feature: [web-terminal.md](../../../../../docs/ft/web/web-terminal.md). (tddy-web)
