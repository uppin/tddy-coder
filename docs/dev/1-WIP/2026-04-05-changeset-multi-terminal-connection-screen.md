# Changeset: Concurrent terminal attachments on Connection screen (State B)

**Date**: 2026-04-05  
**Status**: Complete  
**Type**: Feature (product + technical documentation)

## Affected packages

- `docs/ft/web/` — feature documentation and product changelog
- `docs/dev/` — cross-package changeset index
- `packages/tddy-web/docs/` — `terminal-presentation.md`, `changesets.md`

## Summary

The daemon **ConnectionScreen** holds an unbounded map of **`sessionId` → LiveKit connection parameters**. Operators attach to multiple sessions at once; each attachment uses distinct LiveKit room and identity values. Floating **overlay** / **mini** presentation renders one **`ConnectedTerminal`** per attachment with automation roots **`data-testid="connection-attached-terminal-{sessionId}"`**. **Fullscreen** presentation applies to the session selected by **`focusedSessionIdFromPathname`** for the current **`/terminal/{sessionId}`** path. **Disconnect** removes one entry; navigation back to the session list clears all. Inactive rows from **`ListSessions`** prune only the matching attachment.

## References

- [web-terminal.md](../../ft/web/web-terminal.md)
- [web/changelog.md](../../ft/web/changelog.md)
- [terminal-presentation.md](../../../packages/tddy-web/docs/terminal-presentation.md)
