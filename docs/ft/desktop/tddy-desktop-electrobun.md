# Tddy desktop app (Electrobun) — superseded

Electrobun was removed on **2026-09-05**. The desktop app is a Tauri application that hosts
`tddy-daemon` in its own process, with no child process and no listening socket.

**The current design is [tddy-desktop-tauri.md](tddy-desktop-tauri.md).**

This file remains so that changelog entries and package changesets written before that date keep
resolving. What it used to describe — an Electrobun Bun main process that spawned `tddy-daemon` as a
child, waited for its HTTP port, and talked to it over `127.0.0.1` — no longer exists.

What carried over unchanged: the **Codex OAuth loopback tunnel** is still `tddy-daemon`'s
([codex-oauth-relay.md](../daemon/codex-oauth-relay.md),
[oauth-loopback-tunnel.md](../../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md)), and the
desktop app still neither binds OAuth TCP itself nor joins LiveKit for it.
