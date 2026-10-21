# 2026-10-21 — The start paths of screen sharing are under 60 lines

**Type:** Refactor

`#host-desktops` 10/10 ([#693](https://github.com/uppin/tddy-coder/pull/693)). No behaviour change.

- `start_stream`'s `#[async_trait]` body (67 lines) was moved by hand, with the developer's consent and proved
  byte-identical by script, into `ScreenSharingServiceImpl::start_stream_impl`; the engine then extracted the
  target lookup → password → spawn ladder as `start_session_bridge` and moved both into
  `screen_sharing_service/session_stream.rs` (116). `start_stream` 72 ➜ `start_stream_impl` **41** lines;
  `screen_sharing_service.rs` 482 ➜ 415 production lines.
- `start_host_stream_impl` 76 ➜ **53** lines: the engine extracted `find_host_target` and
  `start_bridge_for_desktop` (the X11 `x11vnc` start and the signed-in spawn) in `host_targets.rs` (346). One
  stray `*` the extract assist wrote before `format!(` was deleted by hand (E0277, a recorded engine defect).

Resolved backlog entry: `2026-10-20-screen-sharing-start-functions-over-length` (together with
`tddy-screenshare`'s `start_bridge`). Left open: `2026-10-20-screen-sharing-two-stand-in-bridges` (no engine
operation reaches it; the developer declined a hand edit). Baseline: 79 passed / 0 failed before and after.
