# 2026-09-26 — The jail boundary distinguishes a dead channel from a failing command

**Type:** Architecture

`WorkspaceSandbox::execute_tool` returns `ToolDispatchOutcome` rather than `ExecuteToolResponse`:
the tool **ran** inside the jail and this is what it answered — `is_error` included, which is a
tool result like any other and says nothing about the jail — or the call **never reached a tool**.

The trait doc previously argued for reporting both alike, and that was defensible until it was the
difference between a repairable session and a dead one. It now states what the distinction is for —
exactly one caller decision, whether to rebuild the jail — and what it must never become: a reason
to run the call on the host worktree the session was jailed away from.

The rebuild itself is not here. It lives in `tddy-session-lifecycle`'s `LocalExecTools`, the only
layer holding the sandbox registry *and* sitting beneath all three dispatch entries; this type holds
neither its spec nor a provisioner.

[docs/ft/daemon/remote-codebase-mode.md](../../../../docs/ft/daemon/remote-codebase-mode.md) §
Workspace tool sandbox · [../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
