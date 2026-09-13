# 2026-09-11 — `paired_agent` sits beside the metadata it reads

`paired_agent` is a pure accessor over `tddy_core::SessionMetadata` — two trimmed optional fields,
no other coupling — and it lives here, beside the type it reads.

It was the one edge in a three-way cycle between the session-file context modules and
`tddy-daemon`'s `split_session`: the context reader asks whether a session records a paired split
agent (a split placement's codebase half is persisted as `workspace` but stands in for a
`claude-cli` agent on another host, so `session_type` alone would wrongly reduce it to the shared
base), and `split_session` in turn reads the context syncer's ports. With the accessor here the edge
inverts the right way: `split_session` depends on
[`tddy-session-files`](../../../tddy-session-files/docs/session-files-service.md), not the reverse,
and every session-file module could move.

Two callers, both naming `tddy_core::paired_agent` directly.

Full record: [../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
