# 2026-07-04 — tddy-github / tddy-daemon

**Category:** Future enhancement
**Source:** cross-daemon-session-token changeset, 2026-07-04

- **Refactor `TelegramOAuthStateSigner` to reuse the generic HMAC signer** — `packages/tddy-daemon/src/telegram_github_link.rs:48-135` hand-rolls the same HMAC-SHA256 sign/verify pattern that the new `SessionTokenSigner` (`packages/tddy-github/src/session_token.rs`) generalizes. Once the session-token signer lands, collapse the telegram state signer onto it.
- **Server-side session-token revocation / denylist** — signed tokens are only bounded by their 5-minute TTL; there is no way to revoke a leaked token before it expires. Add a shared (room-propagated) denylist only if leaked-token containment becomes a requirement.
