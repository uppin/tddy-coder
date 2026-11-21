# 2026-11-21 — `WriteSessionFile` / `ListSessionFiles` are not forwarded to a peer host

**Category:** Known gap
**Source:** `#orchestra` 3/9 (PR #981), validation

An orchestrator held on a host other than the one a call lands on is not served the session-file tools:
`run_session_file_tool` answers from the local sessions base only, and there is no `daemon_instance_id`
forwarding as the worktree and session tools have. The changeset's responsibility is the tools and the
attachment answer on the orchestrator's own host, so it was left out.

**What would close it:** route the two tools through the same peer forward the message tools use when the
caller's session record lives on another daemon, with an acceptance test over two in-memory daemons.
