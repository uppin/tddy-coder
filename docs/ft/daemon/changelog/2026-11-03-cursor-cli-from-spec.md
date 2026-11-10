# 2026-11-03 — Cursor CLI sessions are launched from their tool definition

`#agent-tools` 7/12 ([#796](https://github.com/uppin/tddy-coder/pull/796)).

- **Host and jailed `cursor-cli` sessions render from `cursor.yaml`.** The host start and resume use
  the shared PTY launcher; the jailed start uses the jail launcher. No Cursor flag reaches the
  runner from the daemon, and the Claude env overlay is no longer applied to a Cursor jail.
- **The jail has its own persistent home** at `<tddyhome>/homes/cursor`, seeded once with
  `~/.cursor/auth.json` and `cli-config.json`, and its network is narrowed to Cursor's declared
  hosts.
- **Desktop Cursor hooks now reach the daemon.** On the host they report over the daemon's
  agent-tool socket; in a jail they go over the tool-IPC bridge, which serves the jail's own session
  only.
- **Known gap:** on a server (non-embedded) daemon, which serves no agent-tool socket, a host Cursor
  session's hook reports are dropped. Its activity status does not update until a per-user socket is
  in place ([todo](../../../dev/todo/2026-11-03-server-daemon-agent-tool-socket-per-os-user.md)).
- A host without coding agents installed refuses a Cursor start or resume with `FAILED_PRECONDITION`.
- A host launch registers no MCP server: the user's own `~/.cursor/mcp.json` is left alone.

See [Cursor CLI session § Launch](../cursor-cli-session.md#launch-from-the-cursor-tool-definition).
