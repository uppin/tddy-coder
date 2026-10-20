# 2026-10-19 — The coder serves its room participant through a port

`#satellite` 8/13. `tddy-coder`'s LiveKit mode behaves as before — same flags, identities, rooms, token
lifetime, reconnect and metadata. The coder's library now reaches a room through a participant host, and only
the `tddy-coder` binary names LiveKit; a build without a room transport (the demo) refuses a LiveKit flag with a
logged error naming the identity and URL. See [participant host](../../../packages/tddy-coder/docs/participant-host.md).
