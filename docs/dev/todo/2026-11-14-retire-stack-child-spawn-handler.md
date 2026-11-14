# 2026-11-14 — Retire `StackChildSpawnHandler` and the `spawn-child` toolcall plumbing

**Category:** Future enhancement (planned-scope remainder of `#orchestrators` 10/11, PR #887)
**Source:** `docs/dev/1-WIP/2026-11-10-stack-git-tools.md` — its Affected Packages listed
`tddy-agent-launch` for "`StackChildSpawnHandler` retired in favour of the daemon start path", and
green left the handler in place.

- Since #887, `pr_spawn_child` is a daemon exec tool
  (`tddy_tool_engine::stack_git_tools`, served by `tddy-daemon-rpc`'s `DaemonStackGitTools`), the
  six `tddy-tools` `pr_*` methods are gone, and the `spawn-child` mapping has left
  `tddy-toolcall/src/toolcall/client.rs`. No agent can reach the old relay any more.
- What remains is the daemon-side half of that relay, now unreachable from any tool:
  `StackChildSpawnHandler` (`packages/tddy-agent-launch/src/stack_child_spawn.rs`, its
  `ChildSpawnHandler` impl in `child_spawn_handler.rs`) bound for `pr-stack` sessions in
  `svc_start_claude_cli_session.rs::managed_spawn_handlers`, the
  `tddy_core::toolcall::ChildSpawnHandler` trait and `SpawnChild` wire handling in
  `tddy-toolcall/src/toolcall/listener.rs`, and the `child_spawn_handler` plumbing through
  `tddy-cli-sessions/src/session_toolcall.rs` and `tddy-agent-launch`'s launch steps.
- **Why not in #887:** retiring it reaches `tddy-toolcall`, `tddy-cli-sessions` and
  `tddy-session-lifecycle` — none of them in #887's affected packages. The
  `tddy-session-lifecycle` wiring tests (`stack_child_spawn_tests.rs`,
  `stack_child_spawn_agent_tests.rs`) drive the handler *and* the operator Start-session dialog
  path (`SpawnStackParent::OwnedBy`) in the same fixtures; deleting the handler there without
  re-cutting those tests would drop the dialog path's coverage, which is not #887's to lose.
  Removing only the `tddy-agent-launch` struct would leave the trait, wire arm and plumbing
  orphaned in the other three crates.
- **What would close it:** one follow-up PR over `tddy-agent-launch`, `tddy-toolcall`,
  `tddy-cli-sessions` and `tddy-session-lifecycle` that deletes the handler, the trait and the
  `SpawnChild` wire arm, re-cuts the two wiring suites so the operator-dialog spawn keeps its
  coverage without the handler fixture, and updates the `spawn-child` wording in
  `svc_pr_status_for_caller.rs`, `stack_parent.rs` and `conversation_spawn.rs`.
  Recommendation (taken at #887's wrap, developer instruction "finish without stopping"):
  follow-up PR, not #887.

## Developer decision

**Deferred by the developer at the #orchestrators TODO gate, 2026-11-14** — the follow-up PR is not added to `#orchestrators`; it stays open for a later
change.
