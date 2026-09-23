# 2026-09-23 — Signing in needs no LiveKit block

A desktop install's identity is `github:` + `users:`. The application's daemon signs session tokens
with an Ed25519 key it generates for itself on first launch (`signing_key.pem`, mode `0600`, in
`auth_storage`), so no invented `livekit.api_secret` is needed before a token-gated RPC answers —
one of the three barriers a fresh `./install --desktop` hit. See
[tddy-desktop-tauri.md](../tddy-desktop-tauri.md). The other two are `#keyring` 2/9's
([#509](https://github.com/uppin/tddy-coder/pull/509)).

`#keyring` 1/9, [#508](https://github.com/uppin/tddy-coder/pull/508).
