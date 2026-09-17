# 2026-09-15 — The daemon orphans sandboxed runners on shutdown, and never notices one that died

**Category:** Defect
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset — planning discovery of the
child-process lifecycle, while looking for a pattern the new index daemon could reuse

Two related gaps in how `tddy-daemon` treats `tddy-sandbox-runner`, found while establishing which
parts of that path were worth copying. Neither is caused by the warm-index change; both are reasons
it deliberately does **not** copy this path.

## 1. Shutdown kills CLI sessions and nothing else

`packages/tddy-daemon/src/main.rs:145-168` wires SIGTERM to `daemon.cli_sessions.kill_all()`, and
`:177` calls it again after the server drains. `SandboxSessionState::stop()`
(`packages/tddy-daemon-sandbox/src/sandbox_session.rs:77-85`) — which kills the child and, failing
that, escalates to the process group via `terminate_sandbox_process` (`:687-706`) — is reached from
only three places: `session_coordinate_handlers.rs:654` (DeleteSession), `:662` (the workspace
jail), and `svc_split_context_from_codebase_host.rs:382`.

None of them is the shutdown path. **A sandboxed session's runner survives the daemon that spawned
it.** The workspace jail is partly covered by a `Drop` impl (`workspace_tool_sandbox.rs:458-462`),
but only if that registry is actually dropped rather than the process exiting first.

## 2. Nothing watches a runner for death

`dial_and_bridge` discards the relay's `JoinHandle` (`sandbox_session.rs:418-426`):
`run_host_relay_with_rpc(...).await?; Ok(())`, where that function returns
`Result<JoinHandle<()>, String>` (`packages/tddy-sandbox-runner/src/host_relay.rs:504-510`). The
`SandboxHandle` is moved into `SandboxSessionState` and only `try_wait`ed if someone calls `stop()`.

So death is discovered lazily, per call, when a tool call fails — `exchange_in_jail_tool_call`
surfacing *"its session channel ended"* or *"it did not answer within {}s"*
(`workspace_tool_sandbox.rs:464-496`), after which `execute_tool` sets `*guard = None` permanently
with a comment that is exactly right about why (`:427-445`):

```rust
                // A channel that lost its answer cannot be reused: the next response would be
                // matched to the wrong request.
```

There is no crash detector, no restart policy, no backoff. `relaunch_sandboxed_runner`
(`svc_relaunch_sandboxed_runner.rs:21-39`) exists but is reachable only from the explicit
`resume_sandboxed_claude_cli_session` RPC.

## The contrast that makes this worth recording

`tddy-lsp`'s `LspServerBody` already does both correctly for rust-analyzer, in the same daemon:
it registers the child pid so the `TaskRegistry` escalation can reach it
(`packages/tddy-lsp/src/server_body.rs:75-77`), selects on `child.wait()` so an exit is observed
rather than discovered (`:148-164`), and performs a bounded graceful shutdown before killing
(`:168-173`, `GRACEFUL_SHUTDOWN = 500ms`). `tddy-supervisor` does it correctly too, with a real
`Starting → Running → Backoff → GaveUp` state machine and a `SIGCHLD` drain guarded against
attributing an exit before the pid is recorded (`packages/tddy-supervisor/src/supervisor.rs:484-510`).

So the repo has two good answers and the sandbox path uses neither. Deferred because fixing it
touches session lifecycle and shutdown ordering — the daemon's most load-bearing paths — and the
warm-index changeset has no business rewriting them to land an unrelated feature. It is named in
that changeset's `## Prerequisites` as a pattern deliberately not copied.
