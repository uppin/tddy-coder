# 2026-11-03 — Cursor CLI from cursor.yaml

**Type:** Feature

`#agent-tools` 7/12 ([#796](https://github.com/uppin/tddy-coder/pull/796)).

- `session-hook` picks its transport with `hook_transport`: the tool-IPC socket in a jail, `--daemon-socket` on the host, HTTP otherwise.
