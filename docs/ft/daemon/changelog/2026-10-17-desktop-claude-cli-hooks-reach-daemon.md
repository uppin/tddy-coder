# 2026-10-17 — Tddy Desktop claude-cli sessions report their activity

- **The Activity pane appears for desktop claude-cli sessions.** On Tddy Desktop the session hooks
  posted to the loopback port a GitHub sign-in returns on, which answers every RPC with 404, and the
  hook swallowed the failure. No tool call was recorded, so the Activity control — shown only once a
  session has recorded activity — never appeared, and hook-driven activity status never moved. Hooks
  on the desktop now report over the daemon's agent-tool socket.
- **Split agents' hooks** on the desktop report the same way.
- **Existing sessions keep the hooks they started with**; restarting a session rewrites them.
- **Not yet covered:** cursor-cli sessions on the desktop still report over HTTP and record nothing.

See [claude-cli session](../claude-cli-session.md#which-transport-a-hook-reports-over).
