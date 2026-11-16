# 2026-11-16 — A remote agent's hooks reach the daemon with nothing installed

**Type:** Feature · `#remote-shell` 6/7, PR [#920](https://github.com/uppin/tddy-coder/pull/920)

`remote_hook_command.rs` holds the hook command of a session whose agent runs on a target:
`REMOTE_HOOK_LOG` (`.tddy/hooks.log`) and `remote_hook_command(event, hook_token, log_path)`, a
`printf` one-liner (only `printf`, `cat`, `tr`) appending the event, the token and the payload
to the log in one write. `HookCommandParams.remote_log` selects it; under it the Claude, Cursor
and Muse builders omit the hooks that wait for an answer (Muse's of those names only report,
so Muse keeps all eight), and the Muse and Cursor adapters accept the carrier in place of a
daemon socket. Full record:
[`docs/dev/changesets/2026-11-16-remote-shell-remote-hooks.md`](../../../../docs/dev/changesets/2026-11-16-remote-shell-remote-hooks.md).
