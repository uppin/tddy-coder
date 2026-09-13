# 2026-09-12 — Three duplications node 7 moved rather than removed

**Category:** Future enhancement
**Source:** `#unbundle` node 7, [#476](https://github.com/uppin/tddy-coder/pull/476), changeset
[`2026-09-09-unbundle-session-agent-services`](../changesets/2026-09-09-unbundle-session-agent-services.md)

Each of these existed before the node and survived it unchanged. They are recorded here because the
move is what made them visible as duplications *across crates* rather than within one.

## 1. ACP replay framing, written twice

`packages/tddy-session-activity/src/streams.rs` holds `acp_replay_frame`, `seq_by_tool_call`,
`relay_acp_replay` and `relay_acp_replay_count`. `packages/tddy-coder/src/session_participant/activity_service.rs`
holds `replay_frame_bytes` and `count_frame_bytes` inline inside its dispatch arm, plus its own
`tool_call_id → seq` bookkeeping.

Both read `tddy-service`'s shared `acp_replay` helpers (`tail_page`, `page_before`,
`strip_tool_body`), so the *paging* is one implementation. What is written twice is the framing and
the sequence stamping around it — which is exactly the layer
[`2026-08-02-activities-tail-first-autoscroll`](../../../packages/tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md)
records an incident about: a session that opened tail-first over HTTP and head-first over LiveKit.

Node 6 solved the same shape for the terminal family by having the coder participant register
`tddy-terminal-rpc`'s **own** entry constructor, so there is one implementation. Node 7 did not do
that for activity, and `packages/tddy-coder/tests/two_server_parity_acceptance.rs` is what currently
holds the two in line.

## 2. `fetch_delta`, written twice

`packages/tddy-session-agents/src/session_agent_clone.rs:1070` and
`packages/tddy-session-sync/src/sync.rs:700` both implement "ask for this call's delta and decide
what to do with the answer". The clone mirror already depends on `tddy-session-sync` as a library
precisely so the restore/apply/diverge algorithm is not written twice; this function is the piece
that was.

It matters now for a second reason: the delta **tick numbering** changed in this node, and a rule
implemented in two places is a rule that can be migrated in one.

## 3. Names that no longer describe what they name

Left alone deliberately — renaming these mid-stack cascades conflicts into nodes 8 and 9, which touch
the same files:

- `tddy-daemon`'s `serve_connection_uds` now mounts **six** services, only one of which is
  `connection.ConnectionService`.
- `tddy-daemon/src/connection_service/svc_*.rs` now holds the ports and peer routing for four
  services that are not the connection service.
- `tddy_service::proto::session_agents_svc` carries a `_svc` suffix its six sibling generated modules
  do not, to avoid colliding with the `session_agents` module holding `IN_JAIL_RELAYABLE`.

## 4. `SessionMainPaneProps` takes 42 props

`packages/tddy-web/src/components/sessions/SessionMainPane.tsx`. Node 7 re-pointed several of them at
the new coordinates and added none, but a 42-prop interface is where a coordinate change becomes a
42-line diff. Splitting it is a web restructure with its own Cypress cost and is not a move node's.
