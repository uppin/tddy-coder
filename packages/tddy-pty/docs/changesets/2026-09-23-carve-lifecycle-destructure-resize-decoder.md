# 2026-09-23 — `strip_resize`, the one decoder for a terminal client's in-band resize

**Type:** Refactor

`#carve` 14/15, PR [#524](https://github.com/uppin/tddy-coder/pull/524), DRY #12 (`2c70f1ab`).
Cross-package entry: [2026-09-23-carve-lifecycle-destructure.md](../../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md).

`tddy_pty::resize::strip_resize` (re-exported at the crate root) removes an OSC resize sequence
`\x1b]resize;{cols};{rows}\x07` from a PTY's input and returns `(Option<(cols, rows)>, remaining)`.
It replaces three copies: `tddy-session-lifecycle`'s `cli_session_manager`, `tddy-coder`'s
`session_participant/terminal_manager.rs` (byte-identical), and `tddy-sandbox-runner`'s
`strip_resize_escape` (proven the same function). It lives here because `tddy-sandbox-runner` runs
inside every jail and should depend only on a light crate; this one pulls in only `bytes` and
`tddy-task`.

Not merged, recorded by the sweep: `tddy-tui`'s `parse_resize_from_buf` is a different decoder
(anchored at the buffer start, returns the bytes consumed), and the resize *encoder* has three copies
(`tddy-terminal-rpc`, and two in `tddy-sandbox-app`, which is not a consumer-edit exception).
