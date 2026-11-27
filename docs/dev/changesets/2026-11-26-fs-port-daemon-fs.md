# 2026-11-26 — The daemon crates do disk I/O through the Fs port

**Type:** Architecture · **PR:** [#1047](https://github.com/uppin/tddy-coder/pull/1047) (`#fs-port` 3/13)
**Packages:** `tddy-fs`, `tddy-daemon-kernel`, `tddy-daemon-auth`, `tddy-daemon-sandbox`,
`tddy-daemon-rpc`, `tddy-daemon`, `tddy-daemon-livekit`, `tddy-session-store`

The production disk I/O of the six daemon crates goes through the `Fs` port: each crate's
disk-touching entry points take `Arc<dyn Fs>` (or a struct holding one) and delegate to new
`<fn>_on` seams, with `DiskFs` at composition sites. Every crate carries
`tests/fs_port.rs` — the `direct_disk_io_in` guard over its own `src/` plus one `MemFs`
behaviour test naming the behaviour (user config read, signing-key generation, jail-tree
preparation, session queue listing, socket-dir preparation, scratch-index seeding).

The port grows additively (no existing signature or behaviour changes): `hard_link`,
`create_dir_all_with_mode`, `remove_dir`, `metadata_follow`,
`FsMetadata::{uid, dev, ino}`, `FsKind::Socket`. Both adapters pass the shared conformance
suite extended for the new surface. Consumers matching on `FsKind` handle `Socket` beside
files and symlinks — the session listing skips a socket the way it skips an unreadable
entry. Detail: [the filesystem port](../../../packages/tddy-fs/docs/filesystem-port.md).

File-length gate: `git_work.rs` crossed the 500-production-line budget through this change and
was split here (the op journal moves to `git_work/journal.rs`). `tddy-fs/src/mem.rs` also
crossed it and is recorded as
[`oversized-file-mem.md`](../../../packages/tddy-fs/docs/code-issues/oversized-file-mem.md) —
split deferred because every other `#fs-port` node touches `tddy-fs`; the post-stack sweep
splits it. `session_room.rs` and `workspace_tool_sandbox.rs` grew while already over and carry
regressed rows in their standing records, split deferred for the same sweep; `auth.rs` shrank
579 → 567.
