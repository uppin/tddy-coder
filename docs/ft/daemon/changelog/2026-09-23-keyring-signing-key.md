# 2026-09-23 — Each daemon signs session tokens with a key of its own

- **Session tokens are `v2`: Ed25519-signed, naming the key that signed them.** Each daemon
  generates its keypair on first boot (`signing_key.pem`, mode `0600`, in `auth_storage`) and
  reuses it. A daemon verifies a peer's token against the public key that peer advertises on the
  common room — no secret is shared between daemons. See
  [session-auth.md](../session-auth.md).
- **Authentication no longer depends on LiveKit.** A daemon with a `github:` block and no `livekit:`
  block at all completes a sign-in and serves every token-gated RPC, the settings service included.
  `livekit.api_secret` signs LiveKit room JWTs and nothing else
  ([auth-livekit-services.md](../auth-livekit-services.md)).
- **Breaking: every signed-in client signs in once more.** `v1` tokens are refused, with no
  migration window.
- **Only a daemon's own identity is believed as a daemon.** Peer discovery and every client-facing
  LiveKit mint read one rule, so a signed-in web user cannot join the common room under an identity
  whose advertised key peers would trust
  ([livekit-peer-discovery.md § Trust model](../livekit-peer-discovery.md#trust-model)).
- **The key file is guarded.** A key file other accounts can read is refused at startup, never
  repaired; an `auth_storage` directory looser than `0700` is warned about once.
- **Known trade-off:** a peer's learned key is never evicted, so revoking a daemon means restarting
  the daemons that verify its tokens.

`#keyring` 1/9, [#508](https://github.com/uppin/tddy-coder/pull/508).
