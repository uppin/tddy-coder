# 2026-11-28 — fchmod's mode cast to libc::mode_t

**Type:** Bug Fix · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)

Unrelated build fix riding with the agent-token renewal: `host_socket.rs` casts the
`DIRECTORY_MODE` constant to `libc::mode_t` before passing it to `fchmod`, which takes a `u16` on
macOS — master did not compile there.
