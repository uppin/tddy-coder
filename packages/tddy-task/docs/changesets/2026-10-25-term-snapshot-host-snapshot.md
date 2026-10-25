# 2026-10-25 — Terminal capture keeps a screen model and serializes snapshots
**Type:** Fix

`#term-snapshot` 1/4 ([#760](https://github.com/uppin/tddy-coder/pull/760)).

- `TerminalCapture` feeds every appended byte into an `avt` screen model (new dependency
  `avt = "0.18"`, no scrollback) with incremental UTF-8 decoding: a code point split across chunks is
  held until it completes, invalid bytes render as U+FFFD.
- `resize(rows, cols)`, `grid() -> Option<(u16, u16)>` and `snapshot() -> Option<ScreenSnapshot>`
  (`ScreenSnapshot { data, offset, rows, cols }`). `snapshot()` is `None` until `resize` sizes the
  model. The snapshot is RIS + the `avt` dump rewritten by `src/screen_dump.rs` (C1 CSI written as
  `ESC [`, `?1047` as `?1049`, default-pen blank runs skipped with cursor-forward while a lone
  interior blank is kept) + restore sequences for the input modes; `offset` is `end_offset()`.
- The mode sniffer tracks mouse `1000/1002/1003/1005/1006/1015/1016`, focus `1004`, bracketed paste
  `2004`, the alternate screen `47/1047/1049`, DECKPAM/DECKPNM and DECSCUSR. `mode_prologue` (the
  legacy `TAIL`/`FROM_OFFSET` replay) stays mouse-only, now including `1005`.
- The sequence point: `TaskChannel::write` appends and broadcasts under the capture lock.
- Dev-dependency `vt100 = "0.15"` as the independent reference emulator.

Tests: `tests/terminal_capture_snapshot.rs` (6), `tests/terminal_capture_sequence_point.rs`.
Measured: ~57 MB/s append throughput on synthetic TUI output (release, Apple silicon); ~0.1 ms per
snapshot.

Code issues: new record `oversized-file-terminal-capture` (`src/terminal_capture.rs` 482 → 668
production lines); `complexity-terminal-capture-feed` unchanged at 54 lines (the widened sniffer
did not deepen `feed`). The split is deferred because the file is shared with `#term-snapshot`
2/4–4/4 (developer consent).

See [terminal-capture.md](../terminal-capture.md).
