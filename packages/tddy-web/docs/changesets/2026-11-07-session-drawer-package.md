# 2026-11-07 — Sessions drawer moved to `tddy-session-web` (#web-split 18/20, PR #862)

`components/sessions/` is gone: `index.tsx` renders `SessionsDrawerScreen` from `tddy-session-web`, and
`AppClientsPorts` fills the new `SessionTrafficProvider` with this app's meter registry and
`tddy-livekit-web`'s `liveKitRoomOf`. `test:unit` no longer lists `src/components/sessions`; the unit-test
preload moved to the package (root `bunfig.toml` repointed). Wiring specs stay here (real LiveKit
routing and identities, input forwarded to a bridge participant, real token HTTP, the app shell, selector
and URL store), including two split out of package specs (`PrStackChatNoRoomAcceptance`,
`PrStackChatTokenFailureAcceptance`). Support files no remaining spec reads were pruned;
`inactive-session-activities.md` moved to the package's docs.
