# 2026-11-07 — A pointer whose owner or socket is gone is refused

**Type:** Feature

`#agent-tools` 21/28 ([#829](https://github.com/uppin/tddy-coder/pull/829)).

- `session_source.rs`: `SessionNotFound::UnreadablePointer { path, cause: UnreadablePointerCause }`, the cause `Invalid(String)`, `OwnerGone { owner }` or `SocketMissing { socket }`. A pointer whose owner is on this host is refused when the owner is not running or the socket path is missing; an owner on another host and an ownerless pointer are not checked, nor is `--socket`.
- Tests: `tests/session_source_unit.rs` (owner gone, socket missing with `--socket` not prechecked, ownerless, owner on another host).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-session-pointer-ownership.md).
