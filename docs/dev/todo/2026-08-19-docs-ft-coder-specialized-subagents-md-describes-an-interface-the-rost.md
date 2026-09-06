# 2026-08-19 — `docs/ft/coder/specialized-subagents.md` describes an interface the roster replaced

**Category:** Future enhancement
**Source:** list-subagents-registry-assistants changeset, 2026-08-19

Found while correcting criterion 16 of that document. Only criterion 16 and the architecture diagram
were in this changeset's scope; the rest of the file still describes the pre-roster world and is
misleading to anyone reading it as current:

- **Criteria 17–19 describe `StartSessionRequest.specialized_agents` and the "Managed codebase"
  collapsible multi-select** as the way agents are chosen. The session-agent-roster changeset
  (2026-08-18) replaced both: agents are attached to a live session as a revisioned roster of
  `name@daemon_instance_id`, and `specialized_agents` is gone from the wire.
- **The builtin `fastcontext` def is referenced as a live def source** in the load-time section
  ("or the builtin fastcontext def") and in criterion 16's neighbours. Every hardcoded builtin was
  deleted by the same changeset.
- **`TDDY_SUBAGENT`** appears as a default the jail receives; it was removed too.

Cheap to fix and worth doing as its own documentation pass, since a reader cannot tell which criteria
in the file survived. Whoever does it should reconcile against
[session-agent-roster.md](../../ft/daemon/session-agent-roster.md), which is the current description.
