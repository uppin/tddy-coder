# The filesystem port (`tddy-fs`)

Every disk read and write in tddy goes through `Fs`, so behaviour that depends on disk is driven
against `MemFs` in a test and against `DiskFs` in production. The crate is std-only plus `uuid`
(and `tokio` behind the `tokio` feature), so any crate — including crypto leaves — can depend on
it.

## The trait

`Fs` is object-safe; crates hold it as `Arc<dyn Fs>` (or `&dyn Fs` for one call):

- reads: `read`, `read_to_string`
- writes: `write` (truncating), `append`, `write_atomic` / `write_atomic_with_mode` (below),
  `hard_link` (never replaces an existing link, so a first-boot race keeps one winner),
  `create_new` (exclusive create: fails with `AlreadyExists` when anything is already there, so
  the check and the creation are one atomic step), `create_new_with_mode` (the same at exactly a
  mode, never wider, not even briefly), `copy` (file copy, `IsADirectory` on a directory
  source on both adapters)
- directories: `create_dir_all`, `create_dir_all_with_mode` (mode rides on the creation, never
  wider, not even briefly), `remove_dir` (empty only), `remove_dir_all`, `read_dir` (full
  paths, sorted), `rename`
- metadata: `metadata`, `metadata_follow` (what `execve` sees through a final symlink),
  `FsMetadata` (kind, length, Unix mode, mtime, owning uid, device and inode numbers), `exists`,
  `set_mode`, `canonicalize`
- kinds: `FsKind` covers files, directories, symlinks and unix-domain sockets

## The atomic write

`write_atomic(path, contents, mode)` replaces `path` so a reader sees the whole old file or the
whole new one. Missing parents are created. With `mode` the replacement is created at exactly that
mode (never wider, not even briefly); without it an existing target's mode is carried over. On
failure `path` is untouched and no swap file is left behind.

On disk the write goes through a per-call swap file beside the target
(`.<basename>.<pid>.<uuid>.swap`, created `create_new`), `write_all` + `sync_all`, then `rename`
over the target and a best-effort directory `fsync`. The free functions `write_atomic`,
`write_atomic_with_mode` and `write_atomic_labelled` (which folds the path into the error) are
this operation on `DiskFs`.

## The adapters

- `DiskFs` runs every operation on the process's filesystem.
- `MemFs` holds an in-memory tree of files, directories and modes, with the same atomic-write
  guarantees. `fail_next_write(kind)` fails the next write part-way with that `io::ErrorKind`
  (consumed once, tree left unchanged); `paths()` lists every path the tree holds, sorted.
- `AsyncFs` (the `tokio` feature) runs the port on tokio's blocking pool, as `tokio::fs` does
  with `std::fs`; `sync()` borrows the port underneath for calls that are fine blocking.

`DiskFs` and `MemFs` pass one shared conformance suite, including the atomic-write failure cases,
so whatever a test pins on `MemFs` holds on `DiskFs`.

## Migrating a crate onto the port

An existing free function keeps its signature and delegates to a new `<fn>_on(fs: &dyn Fs, …)`
(or `Arc<dyn Fs>` where the result keeps writing) with `DiskFs`. Tests call the `_on` form with
`MemFs`; callers outside the migrated crate are untouched.

`tddy_fs::testing::direct_disk_io_in(crate_dir)` (the `testing` feature) reports every place
under `src/` where production code reaches the disk without the port — `std::fs`, `tokio::fs`,
or a bare `fs::` path — skipping `#[cfg(test)]` items and comments. Each migrated crate runs it
over itself in `tests/fs_port.rs` and takes `tddy-fs` with the `testing` feature as a
dev-dependency.

## Migrated crates

`tddy-daemon-kernel`, `tddy-daemon-auth`, `tddy-daemon-sandbox`, `tddy-daemon-rpc`, `tddy-daemon`
and `tddy-daemon-livekit` do their disk I/O through the port: guards plus one `MemFs` behaviour
test per crate in each crate's `tests/fs_port.rs`. A socket file in a directory listing is an
endpoint, not a manifest — consumers matching on `FsKind` handle `Socket` beside files and
symlinks (for example the session listing skips it the way it skips unreadable entries).

`tddy-session-files`, `tddy-session-sync`, `tddy-subagent-worktree`, `tddy-session-worktree` and
`tddy-worktree-service` do theirs through the port too: guards in each crate's `tests/fs_port.rs`,
`MemFs` behaviour tests where the contract names them, and the readers, listings, measurements and
mirrors behind `<fn>_on` seams with `DiskFs` only at composition roots (service constructors,
request handlers, and backward-compat wrappers for callers outside the migrated crates). Async
call sites go through `AsyncFs` (including the new `copy` facade); git-subprocess-bound paths
(mirror, conversation worktrees, scratch-index seeding) inject the handle but bind `DiskFs`, since
a `MemFs` could never back the repositories git itself reads and writes.

`tddy-session-activity`, `tddy-usage`, `tddy-log` and `tddy-cli-sessions` do theirs through the
port too: guards in each crate's `tests/fs_port.rs`, the session stores and service behind
struct-held handles (`ActivityPorts.fs`, per-store `Arc<dyn Fs>`, the notification log and watch
store), free functions behind `<fn>_on` seams, and the logger behind its installed `LOG_FS`
handle (DiskFs until `set_log_fs`). What the port cannot express stays off it and is recorded in
the changeset: glob matching and sqlite in `tddy-usage`, `chown`/`dup2` via `libc`, and the
`/proc` zombie check.

`tddy-agent-launch`, `tddy-agent-backend`, `tddy-coding-subagent`, `tddy-agent-skills` and
`tddy-coding-agent-specs` do theirs through the port too: guards in each crate's
`tests/fs_port.rs`, launches behind `LaunchSessions.fs` and threaded `<fn>_on` seams, backends
behind held handles (`TurnFiles`, `UsageTap`, `SpecHeadlessBackend::with_fs`), and the
conversation log appended per line through the port. What the port cannot express stays off it
and is recorded in the changeset: process spawning and socket binding.

`tddy-supervisor` and `tddy-spawn` do theirs through the port too: guards in each crate's
`tests/fs_port.rs`, the supervisor and cgroup broker behind held handles, spawner entry points
behind `<fn>_on` seams. Descriptor-bound operations (socket-directory `fstat`/`fchown`,
child stdio, pipe ends) stay on `libc`/`OwnedFd` and are recorded in the changeset.

`tddy-sandbox`, `tddy-sandbox-app`, `tddy-sandbox-recipes`, `tddy-sandbox-runner`,
`tddy-sandbox-cgroups` and `tddy-sandbox-darwin` do theirs through the port too: guards in
each crate's `tests/fs_port.rs`, materialize/walk/context behind `<fn>_on` seams, the runner
and the app behind installed handle statics. Link creation/reading, ownership and fd acts
stay on `libc` and are recorded in the changeset.

`tddy-workflow`, `tddy-workflow-engine`, `tddy-workflow-recipes`, `tddy-changeset`,
`tddy-tool-engine`, `tddy-task` and `tddy-actions` do theirs through the port too: guards in
each crate's `tests/fs_port.rs`, session-file formats and the artifact writer behind
`<fn>_on` seams, held handles on `BackendInvokeTask`, the hook structs, `SessionWorkQueues`,
`StartLedger` and `LocalShell`. Temp files handed to an external process by path (`curl`
for merge-pr) go through the disk port only and are recorded in the changeset.

`tddy-code-restructuring`, `tddy-code-analysis`, `tddy-index-daemon`, `tddy-lsp`,
`tddy-lsp-executor`, `tddy-semantic-index` and `tddy-bsp` do theirs through the port too:
guards in each crate's `tests/fs_port.rs`, the restructure run state and gates behind
held handles (`Options`, `Workspace`, `PlanStore`, `WorkspaceIndex`), file reads and
writes behind `<fn>_on` seams. The journal append rides `Fs::append` without the fsync
it used to do — a durable-append op is missing port surface, recorded in the changeset.
