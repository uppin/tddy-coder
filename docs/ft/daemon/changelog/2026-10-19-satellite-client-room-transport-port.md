# 2026-10-19 — Client tools join rooms through a port

`#satellite` 7/13. The worktree mirror, the git SSH shim, the terminal relay and the in-session tool client
still join their LiveKit rooms exactly as before — same identities, room names, token lifetimes and waits. Their
libraries now reach a room through a client room port, and only each tool's binary names LiveKit; a build
without a room transport fails a join naming that nothing is wired. See
[room client](../../../packages/tddy-room-client/docs/room-client.md).
