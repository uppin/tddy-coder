# 2026-11-25 — Watches, messages and the session list span every connected host

- An orchestrator watches the sessions it started on another connected daemon as it does its own:
  `started_by_me` covers them, `session_ids` may name a session on another host, and every event —
  messages, questions, permissions, turn ends, endings — names the host it came from
  (`daemon_instance_id`). It is woken by them when idle, and an event another host evicted before
  relaying it is reported as lost, never dropped silently.
- Sessions message each other across hosts, both ways: `PostSessionMessage` / `tddy-tools send`
  find a recipient on another host (or take `--host`); a message to the orchestrator, or a reply, is
  queued while the link is down and delivered once it is back; an unknown host is refused naming
  it. `ReadSessionMessages` names the sender's host.
- `ListSessions` with `include_peers` lists every connected host's sessions, and the web session
  drawer uses it, so inactive sessions on other hosts show with their host badge.
  `ListProjectSessions` rows carry `host_label`.

Feature: [project-concept.md § Watching sessions on other hosts](../project-concept.md#watching-sessions-on-other-hosts) · [§ Messages between sessions](../project-concept.md#messages-between-sessions) · [session-drawer.md § Cross-Host Active Sessions](../../web/session-drawer.md#cross-host-active-sessions)
