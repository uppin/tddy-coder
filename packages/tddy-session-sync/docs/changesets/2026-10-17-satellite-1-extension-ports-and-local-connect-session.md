# 2026-10-17 — attach asks OpenSessionRoom instead of ConnectSession

**Type:** Fix

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

`attach` calls `open_session_room` (public, over the daemon's Connect-HTTP surface) before joining the
session's room, because `ConnectSession` opens no room for a caller that did not arrive over
LiveKit. A daemon whose LiveKit cannot be reached fails the call, naming the room; a daemon without
the room service is reported as `Refused { method: "OpenSessionRoom" }`.

Code issues: `oversized-file-attach` 520 -> 536 production lines (+16); stays open, split deferred with
the developer's consent.

Docs: [mirroring.md](../mirroring.md#opening-the-room-is-asked-for-not-assumed).
