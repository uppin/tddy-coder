# 2026-10-17 — A local session attach no longer depends on LiveKit

- **A LiveKit that is down no longer blocks a local attach.** Attaching to a session from the same
  machine — the desktop window, the daemon's HTTP route, a local socket — answers at once, whether
  LiveKit is unconfigured, answering, refusing connections or never answering. Previously the attach
  waited on, and failed with, the session room's open.
- **Remote callers still hear the failure.** A client that arrives over LiveKit gets its room opened
  before the call answers, and is told, naming the room, when that fails.
- **`OpenSessionRoom` asks for the room by name.** `tddy-session-sync` and `tddy-tools pty-relay
  --livekit-url` use it before joining; it fails naming the room when LiveKit cannot be reached,
  refuses a session this daemon does not facilitate, and fails when the daemon has no LiveKit
  credentials.
- **A locally attached session still becomes reachable over LiveKit**: its room is opened in the
  background and the outcome is reported, not returned.
- **Integration status is readable.** `GetIntegrationStatus` lists each wired integration (LiveKit
  first) as disabled, misconfigured (naming the missing fields), connecting, connected or unreachable
  (with the last failure), and since when. A room that failed to open because of the session rather
  than the server does not mark LiveKit unreachable.
