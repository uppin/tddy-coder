# 2026-11-07 — The supervisor hands each session user an agent-tool socket

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)).

A managed service may declare `agent_tool_sockets: [{user, path}]`. The supervisor creates each as
root, owned by its user (`0700` directory, `0600` socket), and hands it to the daemon after the host
sockets, named `agent-tools.<user>`. It is validated like `host_sockets:` — the user is an allowed,
non-root, plain account; one per user; a directory no other socket shares; the service has its own
`socket:` — and both kinds together are at most 15.

**Upgrade.** A session user with no entry has every session start refused by the daemon. Re-running
`./install` writes the entries from `spawn_policy.allowed_session_users` while the service declares
none; an existing `agent_tool_sockets:` block is left as written, and the users it omits are warned
about. See [tddy-supervisor.md § Per-user sockets](../tddy-supervisor.md#per-user-sockets-handed-to-the-daemon).
