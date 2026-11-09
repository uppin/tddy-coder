# 2026-11-07 — The jail tool-IPC socket is named from the whole session id

**Type:** Fix

`#agent-tools` 21/28 ([#829](https://github.com/uppin/tddy-coder/pull/829)).

- `spec.rs`: `SandboxSpec::short_ipc_socket_path` names the socket `tddy-<FNV-1a 64 of the session id, 16 hex>-<pid>.sock` instead of the id's first ten alphanumerics (a UUIDv7's timestamp), so two sessions minted in the same millisecond get different sockets; no hashing crate.
- Tests: unit tests in `spec.rs` (same-millisecond ids differ, the exact FNV name, a 400-character id stays under 104 bytes).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-session-pointer-ownership.md).
