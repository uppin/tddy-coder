# 2026-09-23 — `runtime::build` owns one signing identity; `CommonRoomKeyDirectory` is the fleet key directory

**Type:** Architecture · `#keyring` 1/9, PR [#508](https://github.com/uppin/tddy-coder/pull/508)
Cross-package entry: [`docs/dev/changesets/2026-09-23-keyring-signing-key.md`](../../../../docs/dev/changesets/2026-09-23-keyring-signing-key.md)

`common_room_key_directory.rs` (new) adapts `tddy_daemon_auth::KeyDirectory` to the advertised strings: keeps the candidate whose SPKI hashes to the id, remembers learned keys across a reconnect, never waits. `runtime::build` resolves the common room once, loads the key when `github:` is set, builds one `SessionTokens` and advertises its key. New suites `common_room_key_trust_acceptance.rs` and `runtime_signing_identity_acceptance.rs`, both admitted to `test_placement`'s list; `unbundle_endpoint`'s module list admits the adapter. Detail: [daemon-endpoint.md](../daemon-endpoint.md).
