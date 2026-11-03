# 2026-11-03 — Cursor CLI from cursor.yaml

**Type:** Feature

`#agent-tools` 7/12 ([#796](https://github.com/uppin/tddy-coder/pull/796)).

- `cursor_from_spec`: `cursor_template_params`, `node_launcher_of`, `configured_cursor_binary`.
- The host Cursor start (`spawn_cursor_from_spec`) and resume launch through `launch_host_pty`; the jailed start (`launch_jailed_cursor`) through `launch_jailed`. `jail_launch::jail_home` resolves the persistent home before lowering, so a placed `{home}` slot renders.
- Without coding agents every Cursor launch is `FailedPrecondition`.

See [Cursor CLI session](../../../../docs/ft/daemon/cursor-cli-session.md).
