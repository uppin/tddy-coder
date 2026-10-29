# Initial discovery: cross-host-wait (`#home-inbox 10/11`)

## Conclusions

- **The gate.** `orchestrator_is_present` (`packages/tddy-session-activity/src/service/question_answers.rs:120`) reads the child's changeset. It returns `false` when `orchestrator_daemon_instance_id` names another host, and otherwise asks `presence.is_active(orchestrator)`.
- **The local check.** `ServedTerminalPresence` (`packages/tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs:125`) answers `get_terminal(id, MAIN_TERMINAL_ID).is_some()`. That is local only.
- **Peer routing.** `PeerRoutedActivity`, in the same file, already classifies a daemon-addressed call with `classify_addressed_daemon_route` and forwards unary calls to a peer: `forward_target` returns `PeerRoute::Forward { peer_instance_id }`. A presence query can ride the same transport. Streams are refused, not forwarded, which is a TODO there and does not matter here.
- **Answers already cross hosts.** `AnswerSessionPermission` and `AnswerSessionQuestion` are target tools, forwarded to the child's host by `#750`'s `daemon_instance_id` forward, and the waiting hook is held on that host.
- **What resolves the TODO.** It names this exact fix: "A peer query 'does daemon X serve session Y's main terminal', asked by `ServedTerminalPresence` …".

## Exploration 1 (2026-10-29)

These were read on `origin/feature/home-inbox/terminal-styles` (`5dd74f09c`):
- `question_answers.rs`: `orchestrator_is_present`
- `svc_activity_ports.rs`: lines 1–30, 125–137 and 156–215
- `docs/dev/todo/2026-10-29-permission-wait-needs-a-same-host-orchestrator.md`
