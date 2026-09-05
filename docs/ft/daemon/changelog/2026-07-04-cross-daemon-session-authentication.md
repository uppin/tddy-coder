# 2026-07-04 — Cross-daemon session authentication

- App session tokens are now stateless HMAC-SHA256-signed tokens (`v1.<payload>.<tag>` carrying the GitHub identity + `iat`/`exp`) signed with the shared `livekit.api_secret`, so a token minted by one daemon is verifiable by every daemon in the room — fixing `invalid or expired session` when switching daemons in the web UI and the silently-broken peer `ListProjects`/`StartSession`/`AddProjectToHost` forwarding paths
- New `RefreshSession` RPC re-mints a fresh token from a valid one; the web client refreshes every 4 min ahead of the 5-min TTL; logout is client-side; with no `livekit.api_secret` configured, auth is fail-closed (minting errors, every token rejected)
- Removed the previous per-daemon opaque-UUID / in-memory-map / `auth-sessions.json` session model (server-side logout and disk persistence are no longer meaningful for stateless tokens)
- Feature: [session-auth.md](../session-auth.md)
