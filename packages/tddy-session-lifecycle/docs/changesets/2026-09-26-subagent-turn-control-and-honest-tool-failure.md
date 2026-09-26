# 2026-09-26 — A dead workspace jail is rebuilt once

**Type:** Feature

`LocalExecTools` gains an `Arc<dyn WorkspaceSandboxProvisioner>` (passed at its one call site, from
the provisioner `DaemonSessionHost` already owns) and a `jail_relaunch` module. On
`ToolDispatchOutcome::TransportFailed` and only on it, `run_exec_tool_locally` removes the session's
registry entry — dropping the last `Arc` tears the jail down — re-provisions, re-inserts and retries
that call **exactly once**, serialised per session.

A tool that ran and exited non-zero never relaunches. A second transport failure in a freshly
spawned runner is reported, not retried again. A rebuild that cannot be made, and a replacement that
dies the same way, answer with the failure: **a jailed session's tools are never run on the host
worktree, on any path.**

Covered by `connection_service/jail_relaunch_unit_tests.rs` against a recording provisioner —
in-crate, because `LocalExecTools::new` is `pub(crate)` and the path under test is the private
`local_agent_codebase_access`, the same reason `workspace_sandbox_roster_dispatch_unit_tests`
already lives there.

[session-service.md](../session-service.md) § The jail rebuild lives here · [../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
