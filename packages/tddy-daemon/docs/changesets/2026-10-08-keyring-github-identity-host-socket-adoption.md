# 2026-10-08 — The daemon adopts the host sockets its supervisor hands it

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`runtime::build` reads the inherited host sockets before the local socket server adopts its own listener
(which clears `LISTEN_*`) and passes them to the session host. `runtime.rs` grew by 5 production lines
(1,776 to 1,781).

Detail: [daemon-endpoint.md](../daemon-endpoint.md#host-session-sockets).
