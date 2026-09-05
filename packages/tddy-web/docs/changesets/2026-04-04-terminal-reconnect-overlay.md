# 2026-04-04 — Terminal reconnect overlay

**Type:** Feature

**`src/components/connection/terminalPresentation.ts`**; **`ConnectionScreen`** presentation state (**`full`** / **`overlay`** / **`mini`**) and **`ConnectedTerminal`** compact layouts; **`appRoutes.ts`** **`terminalDeepLinkSessionPath`**; **`navigatePath`** + **`onNavigate`** for push; first **`ListSessions`** failure **`setError`**. Bun **`terminalPresentation.test.ts`**, **`ConnectionScreen.test.tsx`**; Cypress **`ConnectionScreen.cy.tsx`** (resume vs connect history). Feature doc: [web-terminal.md](../../../../../docs/ft/web/web-terminal.md); dev: [terminal-presentation.md](../terminal-presentation.md). (tddy-web)
