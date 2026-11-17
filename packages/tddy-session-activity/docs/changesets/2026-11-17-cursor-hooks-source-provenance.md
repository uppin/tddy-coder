# 2026-11-17 — Activity provenance derives from the session's own type

**Type:** Fix · PR [#933](https://github.com/uppin/tddy-coder/pull/933)

Correction to [2026-11-16 — Cursor tool events map to agent activity, paired by tool_use_id](2026-11-16-cursor-hooks-reach-the-daemon.md):
the rows' `source` is the session's own recorded type, derived from the value — never
matched against an agent's name, which the session crates' `no_agent_name_gates` scan
forbids. A `cursor-cli` session's rows are sourced `cursor-cli` as recorded there; a
session of any other CLI type is now sourced by its own type too, where it previously
inherited `claude-cli`. A session that recorded no type keeps `claude-cli`.
