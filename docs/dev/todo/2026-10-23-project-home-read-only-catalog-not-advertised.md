# 2026-10-23 — a project-home session is offered the full exec catalog; only `ExecuteTool` refuses

**Category:** Future enhancement
**Source:** `#project-home` 4/7 (`feature/project-home/home-session`, PR #699)

- A project-home session (`SessionMetadata.session_role = "project-home"`) is read-only. The rule
  is **enforced** on the daemon: `ExecuteTool` and `StreamExecuteTool` refuse every tool outside
  `tddy_tool_engine::read_only::is_read_only_tool` with `PermissionDenied`
  (`packages/tddy-daemon-rpc/src/exec_tool/role_guard.rs`).
- It is **not advertised**: the agent still sees `Write`, `StrReplace`, `Delete`, `Shell` and
  `Await` and learns they are refused only when it calls one. The changeset planned
  `tddy-tools` to advertise `read_only_catalog()` for the role; it was not done because
  `ListExecTools` carries no session id, and a managed claude-cli agent's tool list is fixed at
  spawn by `build_claude_allowlist` (`tddy-sandbox-recipes`), which knows nothing of the role.
- A second, related window: `EnsureProjectHomeSession` records the role **after**
  `start_session_core` returns (`StartSessionRequest` has no role field), so between the agent's
  spawn and that write the guard sees an ordinary session. The home session is started with no
  prompt, so the agent is idle in that window in practice, but the guarantee is not structural.
- **Why kept:** the guarantee holds at execution, which is what the PRD requires; advertising is
  an agent-experience improvement, and both fixes need the role on the start request, which is a
  change to `StartSessionRequest` and the spawn path that `#project-home` 4/7's boundary
  ("does not change start/resume for any other session") kept out.
- **What would close it:** a `session_role` field on `StartSessionRequest`, recorded in the
  initial metadata (`starting_session_metadata`) instead of after the start, and passed to the
  claude-cli spawn so `build_claude_allowlist` drops the non-read-only exec tools for the role
  (or a session id on `ListExecTools`, so `tddy-tools` can filter by the caller's role).
