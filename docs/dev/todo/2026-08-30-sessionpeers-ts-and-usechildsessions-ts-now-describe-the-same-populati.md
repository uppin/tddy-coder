# 2026-08-30 — `sessionPeers.ts` and `useChildSessions.ts` now describe the same population

**Category:** Future enhancement
**Source:** session-agent-conversation-tab changeset, 2026-08-30

- Both filter `orchestratorSessionId === current && sessionId !== current`, differing only in return
  shape. The duplication is unchanged by that changeset — but its *justification* did not survive it.
  "Peer agents I spawned from the header" and "child conversations a workflow spawned" used to be
  different populations; the header no longer spawns anything, so both now name the same set, rendered
  twice on screen (the Session agents list with a Switch button, and the child tabs).
- Collapse to one `useChildSessions(sessionId, sessions)` returning `SessionEntry[]`, let each surface
  project what it needs, and delete `src/utils/sessionPeers.ts`. Two call sites and folding
  `sessionPeers.test.ts` into the other's tests.
