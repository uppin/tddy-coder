# 2026-11-07 — `tddy-livekit-web`: connection, presence and room RPC behind ports (#web-split 6/17, PR #843)

**Type:** Architecture change

Adds `packages/tddy-livekit-web` (workspace entry, `bun.lock`, `web-tddy-livekit-web.yml`) with the
LiveKit connection provider, common room, session room connection, room terminal feed, presence, the
rooms screen, and `livekitPorts()`. `tddy-client-web` gains `PresencePort`, `RoomRpcPort` (with a
send-time `useParticipantProbe`), the `MediaPort` declaration, their doubles, and shared helpers
lowered from `tddy-web`. `tddy-web`'s chat, session usage, PR-stack chat and drawer hold a
`RoomHandle` and use the ports. `scripts/livekit-import-boundary.test.ts`: only the LiveKit packages
import `livekit-client`, with a media allowlist that #855 empties. Re-planned mid-node: media
(screen sharing, VNC, presenter room, camera video) split out to #855.
Deviation from the plan's State B: `livekitPorts()` takes no configuration. Whether LiveKit is on is
still decided where the common room is joined (`clientConfig` → `SelectedDaemonProvider` /
`LiveKitConnections`), so the web-side "is LiveKit configured" reading in
[the optional-livekit evaluation entry](../todo/2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md)
is unchanged (`liveKitIsConfigured` itself was already gone).
Package detail: [tddy-livekit-web](../../packages/tddy-livekit-web/docs/changesets/2026-11-07-livekit-wiring-package.md),
[tddy-client-web](../../packages/tddy-client-web/docs/changesets/2026-11-07-livekit-wiring-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-livekit-wiring-package.md).
Product description: [livekit-wiring-package.md](../ft/web/livekit-wiring-package.md).
Deferred: [`tddy-client-web` helper placement](../todo/2026-11-07-tddy-client-web-holds-ui-and-terminal-helpers.md),
[participant-list test ids defined twice](../todo/2026-11-07-participant-list-test-ids-defined-twice.md).
