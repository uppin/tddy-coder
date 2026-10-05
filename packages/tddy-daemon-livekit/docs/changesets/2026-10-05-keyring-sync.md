# 2026-10-05 — `LiveKitPeerTransport`: credential sync rides its own channel

**Type:** Architecture · `#keyring` 6/9, PR [#513](https://github.com/uppin/tddy-coder/pull/513)
Cross-package entry: [`docs/dev/changesets/2026-10-05-keyring-sync.md`](../../../../docs/dev/changesets/2026-10-05-keyring-sync.md)

`LiveKitPeerTransport` implements `tddy-credential-sync`'s `PeerTransport` over the common room's
participant **attributes** (not `metadata`, `livekit_peer_discovery`'s own single-writer channel)
and the existing `forward_to_peer` RPC mechanism. `ListEligibleDaemons`/`StartSession` forwarding
untouched. Detail: [livekit-service.md](../livekit-service.md).
