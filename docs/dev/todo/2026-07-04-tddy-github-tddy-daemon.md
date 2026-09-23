# 2026-07-04 — tddy-github / tddy-daemon

**Category:** Future enhancement
**Source:** cross-daemon-session-token changeset, 2026-07-04

- **Refactor `TelegramOAuthStateSigner` to reuse the generic HMAC signer** — `packages/tddy-daemon/src/telegram_github_link.rs:48-135` hand-rolls the same HMAC-SHA256 sign/verify pattern that the new `SessionTokenSigner` (`packages/tddy-github/src/session_token.rs`) generalizes. Once the session-token signer lands, collapse the telegram state signer onto it.
  **Premise no longer holds (2026-09-23, #508 `#keyring` 1/9):** session tokens are Ed25519-signed
  per daemon (`tddy-github/src/session_token_v2.rs`), so there is no generic HMAC signer left to
  collapse onto; `TelegramOAuthStateSigner` is now the only HMAC signer here. Kept for its owner to
  close or restate — not deleted, because this changeset never claimed it.
- **Server-side session-token revocation / denylist** — signed tokens are only bounded by their 5-minute TTL; there is no way to revoke a leaked token before it expires. Add a shared (room-propagated) denylist only if leaked-token containment becomes a requirement. The per-daemon key cache has the same shape of gap one level up — a peer's *key* is never revoked either; see `2026-09-23-a-learned-peer-signing-key-is-never-revoked.md`.
