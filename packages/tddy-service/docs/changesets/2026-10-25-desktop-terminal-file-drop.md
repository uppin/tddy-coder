# 2026-10-25 — `SessionEntry.reads_host_paths` (field 36)

**Type:** Feature

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md`](../../../../docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md)

`session.proto`: `bool reads_host_paths = 36` — the session's agent reads its host's filesystem directly; meaningful for "this host" only, the daemon named by `daemon_instance_id`.
