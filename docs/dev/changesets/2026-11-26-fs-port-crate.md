# 2026-11-26 — One filesystem port for all disk I/O

**Type:** Architecture
**PR:** #1045 (`#fs-port` n1 of 13; stack `fs-port`, position 1 of 12)
**Packages:** `tddy-fs` (new), `tddy-session-store`, `tddy-credentials`

## State A

Disk access is direct `std::fs` / `tokio::fs` at every call site, so behaviour that depends on
disk can be tested only against a real temp directory. The crash-safe "replace a file without
truncating it" write exists in four copies: `tddy-session-store/src/atomic_file.rs`,
`tddy-credentials/src/atomic.rs`, and the `write_atomically` helpers in `tddy-agent-launch` and
`tddy-task`.

## State B

One filesystem port: new std-only leaf crate `tddy-fs` holding the `Fs` trait, the `DiskFs` and
`MemFs` adapters, the `AsyncFs` facade (the `tokio` feature), and the atomic write as a port
operation. `tddy-session-store::atomic_file` re-exports the port at its old path;
`tddy-session-store` and `tddy-credentials` production code reaches disk only through an injected
port (`_on` forms with `DiskFs` underneath; `SessionVault` holds `Arc<dyn Fs>`). The
`tddy-agent-launch` and `tddy-task` copies stay until their own migration nodes. Per-crate detail:
[`tddy-fs`](../../packages/tddy-fs/docs/changesets/2026-11-26-fs-port.md),
[`tddy-session-store`](../../packages/tddy-session-store/docs/changesets/2026-11-26-fs-port.md),
[`tddy-credentials`](../../packages/tddy-credentials/docs/changesets/2026-11-26-fs-port.md).

Resolves `docs/dev/todo/2026-09-23-atomic-file-leaf-crate.md`
("Two copies of \"replace a secret without truncating it\""): `tddy-fs` is the leaf crate it asks
for.

## Verification

`./test -p tddy-fs -p tddy-session-store -p tddy-credentials` (14 binaries, 0 failures),
`cargo clippy` with `-D warnings` on the same three crates, and the port guard
(`testing::direct_disk_io_in`) over each migrated crate's `src/` in `tests/fs_port.rs`.
Whole-workspace health is CI's answer on the PR.
