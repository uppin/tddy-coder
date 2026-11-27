# 2026-11-27 — Session files go through the Fs port

**Type:** Refactoring · PR [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)

Uploads, staged attachments, host documents, context files and session files read and write
through `tddy-fs`: handlers take the service's `Arc<dyn Fs>` and delegate to `<fn>_on` seams;
`DiskFs` stays at the old names (kept for the lifecycle RPC tests) and at composition sites.
`tests/fs_port.rs` carries the `direct_disk_io_in` guard plus the `MemFs` upload-listing
behaviour tests. Cross-package entry:
[`docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md](../../../../docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md).
