# 2026-08-14 — A claude-cli split agent has no route to its own attachments

**Category:** Future enhancement
**Source:** session-room changeset, 2026-08-14

The session-room changeset puts a copy of a session's attachments on the facilitating daemon and serves
them to session-room participants over `ReadHostDocument` / `StreamReadHostDocument`
(`scope = SESSION_ARTIFACT`, `relative_path = "attachments/{basename}"`). A browser or a second agent
that speaks the RPC surface can fetch them.

A **claude-cli** split agent still cannot. It runs with every native filesystem tool disallowed and
`--strict-mcp-config` (`packages/tddy-daemon/src/split_session.rs:180-193`), so its only route out is
`mcp__tddy-tools__*` → `ExecuteTool`, whose tools are rooted at the worktree with traversal rejected
(`connection_service.rs:4462`, `:8056`). Attachments live under the *session* dir, outside that root.

Closing it means a new exec tool that deliberately reads outside the worktree, which widens the boundary
the split placement rests on — it needs its own changeset and its own review, not a quiet addition to
the dispatch table at `packages/tddy-tool-engine/src/lib.rs:217`.
