# Agent-tool socket per OS user — PRD

**Date**: 2026-11-06
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 26/28 — branch `feature/agent-tools/agent-tool-socket-per-user`

## Affected Features

- [tddy-supervisor.md](../../supervisor/tddy-supervisor.md) — sockets handed over to the daemon
- [cursor-cli-session.md](../cursor-cli-session.md) — host Cursor hooks
- [claude-cli-session.md](../claude-cli-session.md) — Claude hooks, notify and tool transport
- [systemd-install.md](../systemd-install.md) — `supervisor.yaml`

## Summary

A **server** daemon serves the agent-tool socket the way the desktop daemon does, but with **one
socket per OS user**, owned by that user (directory `0700`, socket `0600`). Under `tddy-supervisor`
the supervisor creates these sockets and hands them over, like the host-session sockets (design A,
chosen by the developer on 2026-11-06). Sessions run as user X reach X's socket. A session for a
user with no socket is refused at start, instead of losing its reports silently.

## Background

[2026-11-03-server-daemon-agent-tool-socket-per-os-user.md](../../../dev/todo/2026-11-03-server-daemon-agent-tool-socket-per-os-user.md):
#796 points host Cursor hooks at the agent-tool socket, but only an embedded daemon binds it. On a
server daemon those hook reports are dropped. A single socket for the daemon's own user would be
unreachable (EACCES) for sessions running as other users. The held server binding is on
`wip/agent-tools-cursor-cli-server-socket` (`07128a63e`), with 3 tests.

## Proposed Changes

### What's Changing

- **Supervisor:** `supervisor.yaml` declares agent-tool sockets per user. They are validated like
  `host_sockets` (allowed user, distinct directory, both kinds within `MAX_HANDED_OVER_LISTENERS`)
  and announced as `agent-tools.<user>`.
- **Daemon:** an `AgentToolSockets` registry binds `<data dir>/run-agent-tools/<user>/agent.sock`
  owner-only when it can, adopts inherited `agent-tools.<user>` listeners under the supervisor,
  and serves the tool roster.
- **Consumers:** Claude's and Cursor's hook, notify and tool transports take the socket for the
  session's OS user. An unlisted user under the supervisor is refused at start, naming the setting
  to add.

### What's Staying the Same

- The desktop (embedded) daemon's socket.
- Single-user paths: `./web-dev`, `--user` installs and desktop.
- The host-session socket and its two RPCs.

## Impact Analysis

### Technical Impact

`tddy-supervisor` (config, socket hand-over, FD names), `tddy-daemon` (runtime binding), the
consumer crates (`tddy-agent-launch`, `tddy-session-split`, `tddy-session-lifecycle`,
`tddy-session-tool-client`, `tddy-tools`). Each user takes two handed-over listeners instead of one,
so the per-host user capacity halves.

### User Impact

Host Cursor and Claude sessions on a server daemon report hooks again. A misconfigured user fails
at start with the fix named.

## Implementation Plan

1. Apply the held server binding and its 3 tests.
2. Supervisor protocol: config, validation and hand-over of `agent-tools.<user>`.
3. Daemon registry: bind or adopt per user.
4. Thread the registry to every consumer; refuse unlisted users.

## Acceptance Criteria

- [ ] A session running as user X connects to X's agent-tool socket and is served; user Y cannot connect to it.
- [ ] Under the supervisor, a session for a user with no declared agent-tool socket is refused at start, naming the setting.
- [ ] `supervisor.yaml` validation rejects a duplicated directory and an over-capacity listener count.
- [ ] `./web-dev`, `--user` and desktop still report hooks.
- [ ] Claude's and Cursor's host hook suites pass on a server daemon.

## References

### Affected Features (Complete List)

- `docs/ft/supervisor/tddy-supervisor.md`
- `docs/ft/daemon/cursor-cli-session.md`
- `docs/ft/daemon/claude-cli-session.md`
- `docs/ft/daemon/systemd-install.md`

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-06-agent-tool-socket-per-os-user.md`
