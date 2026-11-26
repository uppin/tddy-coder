# The filesystem port (`tddy-fs`)

Every disk read and write in tddy goes through `Fs`, so behaviour that depends on disk is driven
against `MemFs` in a test and against `DiskFs` in production. The crate is std-only plus `uuid`
(and `tokio` behind the `tokio` feature), so any crate — including crypto leaves — can depend on
it.

## The trait

`Fs` is object-safe; crates hold it as `Arc<dyn Fs>` (or `&dyn Fs` for one call):

- reads: `read`, `read_to_string`
- writes: `write` (truncating), `append`, `write_atomic` / `write_atomic_with_mode` (below),
  `hard_link` (never replaces an existing link, so a first-boot race keeps one winner)
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
