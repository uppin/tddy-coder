# fs-port follow-up: tddy-zed-extensions still on direct disk I/O

`#fs-port` 13/13 (PR #1057) ported every remaining production crate but one:
`tddy-zed-extensions` (117 direct-disk-I/O lines in `src/` per the node's survey) still
reaches disk directly, so its `tests/fs_port.rs` `direct_disk_io_in` guard is still red.

Follow-up: port `tddy-zed-extensions` onto injected `Fs` handles / `<fn>_on` seams
(DiskFs only at composition roots), make the guard green, and record the change as its
own changeset. Until then the crate is a known gap in
`docs/dev/changesets/2026-12-02-fs-port-rest-fs.md`.
