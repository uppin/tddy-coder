# 2026-03-24 — Connection screen session ordering

**Type:** Feature

`tddy-web` **`sortSessionsForDisplay`** (`sessionSort.ts`) orders **`ListSessions`** rows for project tables and orphans: active first, **`createdAt`** descending, **`sessionId`** tie-break; **`sortedSessionsForProject`** computes one sorted list per project accordion row; Bun + Cypress coverage. Feature docs: `docs/ft/web/web-terminal.md`, `docs/ft/web/changelog.md`. (tddy-web)
