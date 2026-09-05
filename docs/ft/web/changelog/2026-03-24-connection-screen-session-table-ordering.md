# 2026-03-24 — Connection screen: session table ordering

- **ConnectionScreen**: Project session tables (`sessions-table-{projectId}`) and **Other sessions** (`sessions-table-orphan`) render rows in a fixed display order: active sessions first, then inactive; within each group, newer **`createdAt`** (ISO-8601) before older; ties and unparsable timestamps resolve by **`sessionId`** lexicographically. Implementation: **`sortSessionsForDisplay`** in `packages/tddy-web/src/utils/sessionSort.ts`, applied after filtering by project or orphan set. **Bun** unit tests (`sessionSort.test.ts`) and **Cypress** component tests assert order when **`ListSessions`** returns a non-canonical sequence.
- **Feature doc**: [web-terminal.md](../web-terminal.md) (Daemon mode: Connection screen).
