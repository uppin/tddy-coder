# 2026-09-23 — The advertisement carries the signing key, as opaque strings

**Type:** Architecture · `#keyring` 1/9, PR [#508](https://github.com/uppin/tddy-coder/pull/508)
Cross-package entry: [`docs/dev/changesets/2026-09-23-keyring-signing-key.md`](../../../../docs/dev/changesets/2026-09-23-keyring-signing-key.md)

`AdvertisedSigningKey { key_id, public_key }` rides the common-room advertisement as `signing_key_id` / `signing_public_key` (both `#[serde(default)]`); `peer_signing_public_keys` and `CommonRoomPeerRegistry::signing_public_keys_for` return every candidate undecoded. Discovery refuses every identity `tddy_service::may_be_daemon_discovery_identity` rules out. `connect_common_room_publish_metadata` takes the `DaemonAdvertisement` whole (7 → 5 params). The dependency-boundary suite is unchanged. Detail: [livekit-service.md](../livekit-service.md).
