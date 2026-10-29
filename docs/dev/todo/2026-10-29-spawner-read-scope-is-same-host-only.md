# 2026-10-29 — A spawner reads only the children on its own host

**Category:** Gap (low). Deferred from `#home-inbox` 11/11 (sweep, #788), and accepted by the
orchestrator at that node's 6e review.
**Source:** `packages/tddy-daemon-rpc/src/exec_tool/project_home.rs`
(`refuse_a_spawner_outside_its_own_sessions`).

`#home-inbox` 11/11 offers a conversation spawner, such as a `grill-me` session,
`ReadSessionActivity`. It is scoped to the sessions whose changeset names the spawner as
`orchestrator_session_id`. The scope check runs on the **caller's** host, before the call is
forwarded to a target host. It reads the target's changeset under the caller's own sessions
directory.

So a spawner's child that runs on **another** host has no changeset there for the check to read.
Reading it stays refused as permission denied, although the spawner started it. That child's
permission prompts still wait the full `claude_cli.answer_wait_secs` before Claude shows its own
prompt.

**Why deferred:** a spawner's children start on the spawner's host by default. Forwarding the scope
check is a change to the cross-host forward path (`project_home_remote`), and the sweep does not own
that path.

## What would close it

Forward the scope check with the call. The caller's host forwards a spawner's
`ReadSessionActivity` to the target's host with the caller's identity, its session id and daemon
instance id. That is how `AnswerSessionPermission` is forwarded already. The **owning** host then
checks its own changeset: `orchestrator_session_id` must be the caller, and
`orchestrator_daemon_instance_id` must be the caller's host. This mirrors `require_orchestrator` in
`tddy-session-activity/src/answers.rs`.

**Test:** a spawner on host A reads the pending `request_id` of its child on host B and answers it.
A session on host B that it did not start is refused there.
