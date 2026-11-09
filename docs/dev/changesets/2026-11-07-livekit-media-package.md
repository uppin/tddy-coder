# 2026-11-07 — LiveKit media behind `MediaPort` in `tddy-livekit-web` (#web-split 11/17, PR #855)

**Type:** Architecture change

Finishes LiveKit isolation: screen-sharing and VNC input forwarding, the presenter room, camera video,
remote-track rendering and the camera preview live in `tddy-livekit-web` behind `tddy-client-web`'s
`MediaPort` (final shape, with a recording `aMediaDouble`). No `tddy-web` file imports
`livekit-client`; `scripts/livekit-import-boundary.test.ts` runs without its media allowlist.
Behaviour change: the roster's camera preview attaches the participant's camera track (before, the app
lent a dialog with no track, so it always said "No camera track").
Deviations from the plan: the input-forwarding, fullscreen, VNC and presenter-room specs stay in
`tddy-web` unchanged (they reach the LiveKit media port through `AppClientsPorts`); only
`ParticipantListVideoPreview.cy.tsx` moved. `shouldShowParticipantVideoAffordance` and its one unit
assertion were deleted — the rule is `MediaPort.useHasCameraVideo`, asserted by `ParticipantList.cy.tsx`
and the port tests. No package `.yml` caller existed for this node.
Package detail: [tddy-livekit-web](../../packages/tddy-livekit-web/docs/changesets/2026-11-07-livekit-media-package.md),
[tddy-client-web](../../packages/tddy-client-web/docs/changesets/2026-11-07-livekit-media-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-livekit-media-package.md).
Product description: [livekit-wiring-package.md](../ft/web/livekit-wiring-package.md).
