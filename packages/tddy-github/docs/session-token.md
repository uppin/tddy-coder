# Session tokens — the `v2` format

**Module**: [`src/session_token_v2.rs`](../src/session_token_v2.rs) · re-exported at the crate root
(`tddy_github::{SessionTokenSigner, SessionTokenVerifier, SessionClaims, KeyId, …}`)

A session token is the credential a signed-in client presents on every daemon RPC. It is
**stateless**: nothing is stored server-side, and any daemon that knows the signer's public key can
verify it. Each daemon signs with an **Ed25519 key of its own**; no secret is shared between daemons.

This crate owns the *format* only. Key material — generating, persisting and loading the keypair —
and key distribution — learning a peer's public key — belong to `tddy-daemon-auth`
(`DaemonSigningKey`, `KeyDirectory`), which depends on this crate and so cannot appear in its
signatures.

## Wire format

```
v2.<base64url(json claims)>.<base64url(ed25519 signature)>
```

- Base64url is **unpadded** (`URL_SAFE_NO_PAD`).
- The signature covers the ASCII bytes of `v2.<base64url(json claims)>` — everything before the last
  `.` — and is 64 bytes (86 characters).
- The claims:

| Field | Type | Meaning |
|---|---|---|
| `kid` | string | The key that signed this token — see [Key id](#key-id) |
| `id` | u64 | GitHub user id (`0` for an identity the daemon resolved locally, e.g. over its Unix socket) |
| `login` | string | GitHub login — what `users:` maps to an OS user |
| `avatar_url`, `name` | string | Display fields |
| `iat`, `exp` | u64 | Issued-at and expiry, Unix seconds |
| `kind` | `"access"` \| `"refresh"` | Which credential this is |

## Key id

`kid` is **derived from the public key**, never assigned:

```
kid = base64url_nopad( SHA-256( SPKI DER of the Ed25519 public key )[0..16] )
```

22 characters. Because it is a digest of the key, two daemons cannot collide on one, a daemon cannot
change its id without changing its key, and a key resolved for a `kid` can never go stale — a
verifier may cache it forever. `KeyId::parse` accepts only that exact shape: 16 bytes in canonical
unpadded base64url, so one key can never be named two ways.

The same SPKI DER (`session_token_v2::spki_der`) is what a daemon publishes as its public key.

## Signing

`SessionTokenSigner::new(signing_key)` takes the daemon's `ed25519_dalek::SigningKey` and derives the
`KeyId` from its public half, so a signer can never stamp an id that is not its own. `mint_access` and
`mint_refresh` mint the two credentials with their fixed lifetimes; `mint_kind_with_issued_at` is the
general seam, with a clock argument so a test can mint an already-expired token without sleeping.

| Credential | Lifetime | Used for |
|---|---|---|
| access (`SESSION_TOKEN_TTL`) | 5 minutes | Every RPC |
| refresh (`REFRESH_TOKEN_TTL`) | 7 days, slid forward on each refresh | `RefreshSession` only |

The `kind` is enforced in both directions by the callers: a refresh token never authenticates an RPC,
and an access token never mints.

## Verifying

Verification is **two steps with a lookup between them**, and the lookup is not this crate's:

1. `SessionTokenVerifier::key_id_of(token)` reads the `kid` **without trusting it** — it only says
   which key to fetch.
2. The caller resolves that `kid` to a public key: its own key, or one a peer published
   (`tddy_daemon_auth::DirectorySessionTokenVerifier` does this through its `KeyDirectory`).
3. `SessionTokenVerifier::verify(token, &key, now)` checks that the claims' `kid` *is* that key's id,
   checks the signature (`verify_strict`), then the expiry.

`SessionTokenAuthority` is the one-method trait (`async fn verify(&self, token)`) that wraps all three
for a caller that does not care how the key was found — `AuthServiceImpl` verifies through it.

A token is minted the same way whichever GitHub sign-in flow completed — the redirect flow's
`ExchangeCode` or the device flow's `PollDeviceLogin` — because both go through one
`AuthServiceImpl::complete_login`. See [device-flow.md](./device-flow.md).

## Rejections

`SessionTokenError`:

| Variant | When |
|---|---|
| `Malformed` | Not `<version>.<payload>.<signature>`; a segment that is not base64url / JSON; claims missing a field, `kind` included; a signature that is not 64 bytes; a `kid` that is not a key id |
| `UnsupportedVersion` | A well-formed token of another version — a `v1` token, most of all. Reported by version rather than as a bad signature, so a rollout is diagnosable |
| `InvalidSignature` | The signature does not verify, or the key supplied is not the key the token names |
| `Expired` | Correctly signed, but `exp` has passed |
| `UnknownKeyId(kid)` | Well-formed, but no key is known for its `kid` — the one rejection a later retry can fix (the signer may not have announced itself yet) |

There is **no fallback** anywhere in this path: an unknown key is refused, not tried against some
other key, and a `v1` token is refused, not migrated.

## `v1`, retired

`v1` was `v1.<payload>.<32-byte HMAC-SHA256 tag>`, keyed on `livekit.api_secret` — one secret every
daemon in a deployment held. It is not implemented: a `v1` token is `UnsupportedVersion`, with no
migration window, so a client still holding one signs in again.
