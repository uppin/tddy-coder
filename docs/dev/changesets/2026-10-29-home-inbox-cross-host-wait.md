# 2026-10-29 — A child waits for an orchestrator on another host

**Type:** Feature

`#home-inbox` 10/11 ([#787](https://github.com/uppin/tddy-coder/pull/787)). Resolves the backlog
record `docs/dev/todo/2026-10-29-permission-wait-needs-a-same-host-orchestrator.md` (deferred from
#771: a child whose changeset named an orchestrator on another host got `NO_ORCHESTRATOR` at once),
deleted here. Across:

- `tddy-service` — `rpc ActivityService.IsSessionTerminalServed({session_id}) -> {served}`; TS
  regenerated. A new RPC because nothing existing answers "this daemon serves session X's main
  terminal": `ListSessions.is_active` is pid liveness and needs a user token, `ReadSessionTerminal`
  needs a home-session token, the terminal service is stream-only.
- `tddy-session-activity` — `OrchestratorPresence::is_active(session_id, Option<&str>)`;
  `PEER_PRESENCE_TIMEOUT` (3 s) beside it. `orchestrator_is_present` folds no/empty/this host into
  `None` and passes another host on, instead of refusing it. The peer-side handler is refused unless
  `RequestTransport::LiveKit` (the RPC carries no token) and answers from this daemon's terminals only.
- `tddy-session-lifecycle` — `ServedTerminalPresence` holds a `DaemonSessionHost` and, for a peer,
  `peer_serves_main_terminal`: roster classification, then `forward_to_peer_within(.., 3 s)`. Every
  failure is absent, logged at warn. `PeerRoutedActivity` delegates the query locally, never routed.
- `tddy-daemon-rpc` — 7 two-host acceptance tests (`cross_host_answer_wait_acceptance`); test support
  gained `never_answers`, `times_asked` and an `IsSessionTerminalServed` arm. Unit:
  `tddy-session-activity` `cross_host_presence_unit` (8).

Decisions: a local route reached through `Some(this host)` is answered locally, not as absent; the
parent's fake presence (`permission_answer_acceptance`) answers per host. Review verdicts at wrap:
the transport is stamped by the listener's `ServerEngine`, so a local caller cannot claim LiveKit,
but any LiveKit room participant (not only a peer daemon) can ask — accepted, it learns one boolean
about a session id it already holds. The peer is asked once per hook, before the waiter opens; a peer
that answers "served" and then dies costs the hook `answer_wait_secs` and ends `TIMED_OUT` —
accepted, the deadline holds. Peer-failure logging raised from debug to warn at wrap.

Code issues: none added. Production lines: `svc_activity_ports.rs` 477, `question_answers.rs`
216, `rpc.rs` 385 — all under the 500 budget; not split (the first two are also touched by #788).
