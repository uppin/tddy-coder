# 2026-11-06 — The session pointer carries the notify session

**Type:** Feature

`#agent-tools` 17/19 ([#823](https://github.com/uppin/tddy-coder/pull/823)).

- `ResolvedSession.socket` is `Option<PathBuf>` and `ResolvedSession` gains `notify`. The daemon's CLI launch writes a pointer with no socket for a session whose agent only notifies.
- `session_environment(session)` lists what `adopt_session` sets. From the pointer's `notify` map it takes only `notify_env`'s five names, so a pointer cannot set `PATH`. `tddy-tools notify` therefore reads its session from the pointer when its environment names none.
- Tests: `tests/session_source_unit.rs::a_pointers_notify_map_sets_only_the_names_tddy_tools_notify_reads`; `tddy-tools-cli/tests/notify_cli_acceptance.rs::notify_reads_its_session_from_the_pointer_when_its_environment_names_none`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-cli-dispatch-collapse.md).
