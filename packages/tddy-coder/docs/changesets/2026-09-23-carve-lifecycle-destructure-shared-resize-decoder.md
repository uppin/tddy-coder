# 2026-09-23 — The session participant decodes terminal resizes with `tddy_pty::strip_resize`

**Type:** Refactor

`#carve` 14/15, PR [#524](https://github.com/uppin/tddy-coder/pull/524), DRY #12 (`2c70f1ab`), a
consumer edit the developer accepted (2026-09-23). Cross-package entry:
[2026-09-23-carve-lifecycle-destructure.md](../../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md).

`session_participant/terminal_manager.rs` drops its byte-identical copy of the OSC resize decoder
and calls `tddy_pty::strip_resize`, the one definition the daemon's CLI sessions and the sandbox
runner share. No behaviour change.
