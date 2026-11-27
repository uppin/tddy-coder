# 2026-11-28 — Activity, usage and logs go through the Fs port

**Type:** Refactoring · **PR:** [#1050](https://github.com/uppin/tddy-coder/pull/1050) (`#fs-port` 6/13)
**Packages:** `tddy-fs`, `tddy-session-activity`, `tddy-usage`, `tddy-log`, `tddy-cli-sessions`
**Base:** `feature/fs-port/session-files-fs` at `b7e4941d2` (n5, itself on master `681d10612`)

The production disk I/O of the four activity crates goes through the `Fs` port. Session stores,
the notification log and relay, the watch store and the activity service hold injected handles
(`ActivityPorts.fs`, per-store `Arc<dyn Fs>`), free functions delegate to `<fn>_on` seams, and
the logger appends through its installed `LOG_FS` handle (`DiskFs` until `set_log_fs`), with
`DiskFs` only at composition roots: service/store constructors, daemon composition sites
(`tddy-session-lifecycle`), and backward-compat wrappers kept for callers outside the migrated
crates. Every crate carries `tests/fs_port.rs` with the `direct_disk_io_in` guard over its own
`src/`; `tddy-session-activity` additionally carries the contract's `MemFs` answer-wait
round-trip test.

The port grows additively (no existing signature or behaviour changes): `Fs::create_new_with_mode`
— exclusive create at exactly a mode, never wider, not even briefly — with `DiskFs` + `MemFs`
implementations and conformance tests. The tmux pane-env file carries credentials and must be
`0600` from creation; `create_new` + `set_mode` would leave a umask window, and `write_atomic`
would replace rather than refuse an existing file. Detail: [the filesystem
port](../../../packages/tddy-fs/docs/filesystem-port.md).

Known gaps (all recorded here rather than silently left): glob matching and the sqlite source
stay on their crates in `tddy-usage` (the port has no glob and no database operation);
`chown`/`dup2` call `libc` directly in `tddy-cli-sessions`/`tddy-log` (process-credential acts
with no port operation); `pid_is_zombie` reads `/proc` through `DiskFs` (the host's process
table, not session disk). Test-only crates keep real disk.
