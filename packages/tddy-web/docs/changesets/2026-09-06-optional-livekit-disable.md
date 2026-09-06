# 2026-09-06 — The common-room switch (`livekit.enabled`)

**Type:** Feature

Node 8 of the `optional-livekit` stack ([#449](https://github.com/uppin/tddy-coder/pull/449)).
See the cross-package entry for the whole change:
[docs/dev/changesets/2026-09-06-optional-livekit-disable.md](../../../../docs/dev/changesets/2026-09-06-optional-livekit-disable.md).

`GetClientConfig` / `/api/config` carry `livekit_enabled`, mapped in both the RPC and HTTP branches
of `loadClientConfig`, and threaded to `useLiveKitHostDirectorySource`. An explicit `false` withholds
the room coordinates so `useCommonRoom` short-circuits on the guard it already had: no token minted,
no `Room` constructed, and the LiveKit source reporting `idle` rather than `error`. A daemon too old
to report the field joins as it always did. The daemon settings screen renders the switch as a
checkbox (`daemon-settings-livekit-enabled`).
