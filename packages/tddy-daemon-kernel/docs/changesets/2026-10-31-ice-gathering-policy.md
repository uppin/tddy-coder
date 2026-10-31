# 2026-10-31 — `livekit.ice_gathering` configures the ICE gathering policy

**Type:** Feature

`LiveKitConfig` gained `ice_gathering` — `gather_once` (the default) or
`gather_continually`, serde `snake_case` — because the hard-coded `GatherOnce` from the
same-day leak fix took a genuine trade away: continual gathering is the only mode in which
an established session survives a network switch without a reconnect cycle. The default
stays `gather_once`, since the SDK's continual re-gathering leaks the superseded ICE
sockets on every network change (livekit/rust-sdks#1512); the field's documentation carries
the trade-off for an operator choosing `gather_continually` knowingly. Two tests pin the
default and the YAML parsing of both values.

Cross-package entry: [The ICE gathering policy is an operator choice](../../../../docs/dev/changesets/2026-10-31-ice-gathering-policy.md).
