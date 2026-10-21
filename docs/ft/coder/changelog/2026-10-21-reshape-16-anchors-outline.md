# 2026-10-21 — a server that never started says why; static `check` checks an item anchor's module

`#reshape` 16/24. A cold `restructure anchors` (or any cold run) whose rust-analyzer never comes up now names
the program and the reason — not found, could not be spawned, exited before the `initialize` handshake, or the
handshake failed — where it printed the bare `lsp server exited`; the index daemon reports such a server as
unavailable. A plain `check` refuses an item anchor whose crate or module prefix does not match its file, in
the deep resolver's words, and otherwise says the item needs `check --deep`. The v2 plan header drops the
never-read `modified` time; older headers still parse. See
[Rust code restructuring § Plan format](../rust-code-restructuring.md).
