# 2026-09-09 — The identity boundary and the LiveKit service

The daemon's identity boundary is a component of its own: everything that signs a session token,
verifies one, or holds a credential on an operator's behalf — the GitHub OAuth exchange, the token
store, session and LiveKit token minting, and the Codex OAuth loopback tunnel. One function in it
answers "who is this token", and every other service on the daemon answers with that one function
rather than its own.

Everything the daemon does with LiveKit is likewise one component: the per-session room, common-room
peer discovery, the supervisor that keeps the common room joined, and the rooms roster the web's
LiveKit panel reads.

**One RPC coordinate moved.** `StreamLiveKitRooms` is served as `livekit.LiveKitService`;
`connection.ConnectionService` keeps the other 72. Auth's four services were already their own, so
no login, token mint or OAuth flow changed on the wire. A `tddy-web` bundle and a daemon must come
from the same side of that one method; nothing else is affected.

**The secret that signs a room JWT is the same one that signs a session token**, and separating the
two components does not separate it — neither derives a signer of its own. That is now enforced
structurally rather than by convention: LiveKit reaches minting through a port, and a test proves
it cannot reach the identity component at all.

**Credentials at rest are written atomically.** The GitHub token store stages and renames through
the shared helper, in the permission-aware mode, so a crash mid-write leaves the previous value
intact and a first write cannot publish a world-readable credential. One consequence deserves an
operator's attention: an `auth_storage` directory that is already group- or world-readable is no
longer re-tightened to `0700` on every write.

What each component is, and where its seam is drawn:
[auth-livekit-services.md](../auth-livekit-services.md).
