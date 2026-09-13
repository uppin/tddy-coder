# 2026-09-09 — Seventeen more rpcs leave, and the daemon keeps the routing

`connection.ConnectionService` goes from 50 methods to **33**. Family B's nine now answer only at
`session_agents.SessionAgentService` and families M and N's eight at `activity.ActivityService`; the
delegating shim that briefly served both coordinates is deleted, so there is no second address a
client can still call.

Six modules and roughly 3,000 net lines leave `src/`. What stays is the part only a daemon can do:

- **Peer routing.** `svc_session_agent_ports.rs` and `svc_activity_ports.rs` build the ports each
  crate is constructed from and hold the `daemon_instance_id` fork, preserving the existing
  invalid-argument-on-unroutable and local-on-empty semantics exactly. `ReportAgentCloneState` is
  deliberately **not** routed — its `daemon_instance_id` names the *reporting* daemon, not a
  destination.
- **The ports hold `ConnectionServiceImpl` by value**, not `Arc<Self>`, avoiding `self_arc()` —
  which panics when `set_self_handle` was never called, and would have panicked on all 17 handlers in
  every test that constructs the service directly.
- **`session_list_enrichment.rs`** (1,740 lines) stays, reached through a `SessionLabels` port,
  because it serves `ListSessions` — family C.
- **The Telegram notification subscriber** stays, implementing `tddy-session-activity`'s trait across
  the crate boundary.
- **All sixteen acceptance suites** for these subsystems stay, each pinned by `ConnectionServiceImpl`
  or `test_util::{test_service, TEST_TOKEN}`.

`local_socket_server.rs` now mounts **six** tonic services on one `Server::builder()`, bundled in a
`LocalSocketServices` struct rather than eight positional arguments. Both new adapters are generated.
The two tests that claimed to cover that mount were deleted: one read the file as *text* and looked
for a type name a bare `use` satisfies with no mount at all, and its sibling asserted three filenames
are absent — true in exactly the state the first exists to catch. `local_token_uds.rs` now makes real
calls over the real socket instead, and both were demonstrated failing with the `add_service` calls
removed.

`session_agent_clone.rs` keeps a 24-line shim for `clone_worktree_path`. Its module doc used to say
that function is why the file stayed; it is not — **the function has no callers and had none on the
base either**, and `ConnectionServiceImpl::agent_clone_worktree_path` is a different function and is
what the suites drive. Left in place, because pre-existing dead code is not a move's to delete, but
the doc now says so.

24 moved log lines carried no `target:` and so had silently dropped out of
`RUST_LOG=tddy_daemon=debug`. Each now names the daemon module it came from.

Docs: [connection-service.md](../connection-service.md) — 5 endpoint rows and the agent-activity
section moved out, the sibling-service table gained two rows.
Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
