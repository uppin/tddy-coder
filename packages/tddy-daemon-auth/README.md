# tddy-daemon-auth

The daemon's identity boundary: who a session token belongs to, and every credential the daemon
holds on that person's behalf.

## Quick Start

### Build
```bash
cargo build -p tddy-daemon-auth
```

### Test
```bash
cargo test -p tddy-daemon-auth
```

## What it serves

| Service | Methods |
|---|---|
| `auth.AuthService` | 5 |
| `auth.LiveKitTokenService` | 1 |
| `token.TokenService` | 2 |
| `loopback_tunnel.LoopbackTunnelService` | 1 |

All four were already their own protos before this crate existed, so no wire coordinate moved when
the code did and no client migrated.

The crate's one function that matters to everything else is `build_auth_entries`. Its
`user_resolver` is the daemon's single identity function — every other service, in every other
crate, authenticates with a clone of it.

## Two rules this crate exists to hold

**It never depends on `tddy-daemon`.** `tests/dependency_boundary_unit.rs` walks the transitive
manifest closure and fails if it ever does. A re-export would defeat the split, so the test proves
the extraction rather than assuming it.

**One key signs one thing.** Each daemon signs session tokens with an Ed25519 key of its own
(`DaemonSigningKey`, `signing_key.pem` at mode `0600` in `auth_storage`, generated on first boot and
reused after), and `config.livekit.api_secret` signs LiveKit room JWTs and nothing else. A token names
its signer's key; a daemon verifies a peer's token through the `KeyDirectory` **port**, which this
crate declares and `tddy-daemon` implements over the common room — so `tddy-daemon-livekit` never
reaches this crate, and a daemon with no fleet (or no `livekit:` block) still signs and verifies.
Every signer in the daemon comes from its one `SessionTokens`: **no path may construct a second
key**, which would be an identity no peer was told about.

## Documentation

### Technical implementation (how)
- [The identity boundary](./docs/auth-service.md) — what is here, the signing key and the key directory, and secrets at rest
- [`codex_oauth_relay`](./docs/codex-oauth-relay.md) — authorize-URL validation and callback parsing
- [OAuth loopback tunnel](./docs/oauth-loopback-tunnel.md) — the operator TCP listener and its LiveKit bridge
- [Changesets](./docs/changesets/) — applied changeset history

### Product requirements (what)
- [Codex OAuth relay](../../docs/ft/daemon/codex-oauth-relay.md)
- [Host and worktree services](../../docs/ft/daemon/host-worktree-services.md) — why the daemon is split at all

### Neighbours
- [`tddy-daemon-livekit`](../tddy-daemon-livekit/README.md) — mints room JWTs through a port, and carries each daemon's advertised signing key as opaque strings
- [`tddy-github` session tokens](../tddy-github/docs/session-token.md) — the `v2` token format these keys sign
- [`tddy-daemon-kernel`](../tddy-daemon-kernel/docs/daemon-kernel.md) — where `SessionUserResolver` is defined
- [`tddy-host-service`](../tddy-host-service/docs/host-service.md) — the host-key path, which lives there and not here
