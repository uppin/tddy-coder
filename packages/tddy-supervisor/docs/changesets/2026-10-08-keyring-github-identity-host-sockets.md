# 2026-10-08 — The supervisor declares each session user's host socket and hands it to the daemon

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

A managed service lists `host_sockets: [{user, path}]` in `supervisor.yaml`; the root supervisor creates each
socket (`host_socket.rs`: `O_NOFOLLOW|O_DIRECTORY`, `0700` / `0600`, owner moved last) and passes the
listeners from descriptor 4 on with `LISTEN_FDNAMES` (`handover.rs`). **Built and unit-proven; not run under a
real root supervisor** — backlog entry "the supervisor-declared host socket has never run under a real root
supervisor" (2026-10-08). `supervisor.rs` grew from 948 to 965 production lines.

Detail: [host-sockets.md](../host-sockets.md).
