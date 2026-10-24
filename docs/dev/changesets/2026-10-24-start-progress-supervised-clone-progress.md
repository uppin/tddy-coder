# 2026-10-24 — Supervised clones report progress and their own errors

**Type:** Feature · PR [#739](https://github.com/uppin/tddy-coder/pull/739) (`#start-progress` 6/8)

`tddy-supervisor` gains its first server-streaming RPC, `CloneRepository` (`Started{pid}`, progress
lines, `Exited{code, signal, stderr_tail}`): `git clone --progress` as the target user under the
unchanged user/tool/env policy, into a random `.partial-*` sibling renamed on success. `tddy-spawn`'s
`clone_repo_via_supervisor_with_progress` feeds the stream through node 5's parser and throttle and hands
out `CloneHandle::through_supervisor` (stops via `StopSession`). The 25 ms `SessionStatus` polling path,
`clone_request*`, `program_in_path` and `resolve_git_program` are removed. Docs:
[tddy-supervisor](../../ft/supervisor/tddy-supervisor.md),
[architecture § Streamed clones](../../../packages/tddy-supervisor/docs/architecture.md#streamed-clones),
[clone-run § Supervised clones](../../../packages/tddy-spawn/docs/clone-run.md#supervised-clones).

Decisions:

- **A dedicated RPC**, not output inheritance on `SpawnSession`: sessions keep their no-output policy.
- **git resolved on the supervisor's own `PATH`**, still checked against `allowed_tool_paths`; the
  request carries no tool path, so authorization is unchanged.
- **The supervisor owns temp + rename**, so the unprivileged daemon never writes the user's repos dir.
- **`CloneExited.signal` is never set** (the session table has no signal); `code: None` = signalled,
  documented in the proto.
- `CreateProject` / `AddProjectToHost` keep the no-callback `clone_repo_via_supervisor` (streaming
  underneath); they report no progress by design.

Deferred: the daemon-side test —
[docs/dev/todo/2026-10-24-supervised-clone-daemon-side-test-needs-a-running-supervisor.md](../todo/2026-10-24-supervised-clone-daemon-side-test-needs-a-running-supervisor.md).
Code issues: `spawn_broker.rs` 1549 → 1595, `supervisor.rs` 972 → 1019, `server.rs` 708 → 796
production lines (new record); splits deferred with the developer's consent (stack-shared files).
