# 2026-12-02 — sandbox-app call sites compile against the Fs port's signatures

**Type:** Refactoring · PR: the fs-port follow-up

`sandboxed_session.rs` and `spawn.rs` pass `&Path` where the port's `create_dir_all`
and `canonicalize` take it, and `host_agent.rs` hands `write_atomic` the bytes it
already has. Cross-package entry:
[`docs/dev/changesets/2026-12-02-fs-port-zed-extensions-follow-up.md`](../../../../docs/dev/changesets/2026-12-02-fs-port-zed-extensions-follow-up.md).
