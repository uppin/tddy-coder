# 2026-10-24 — A sandboxed main is refused because the sandboxed claude-cli start drops `session_role`

**Category:** Future enhancement
**Source:** 2026-10-24-project-home-from-new-session-form

- `EnsureProjectHomeSession` with a `start` spec refuses `sandbox` (and `sandboxed_codebase`) with
  `INVALID_ARGUMENT` (`refuse_what_main_cannot_honour`,
  `packages/tddy-agent-launch/src/project_home_start_spec.rs`, which carries the code `TODO`).
- **Why deferred:** the sandboxed claude-cli start
  (`start_sandboxed_claude_cli_from_request`, `packages/tddy-agent-launch/src/cli_branch_starts.rs`)
  passes `repo_path` and `managed_codebase` but not `session_role`. Only the unsandboxed
  `start_claude_cli_from_request` carries it. A sandboxed main would therefore run without the
  `project-home` role in its record, and the daemon would read it as an ordinary session: no
  read-only refusal in `ExecuteTool`, no project-home tools, no `--allowedTools`. That is exactly the
  state the role exists to prevent, so the placement is refused rather than started unguarded.
  Approved by the developer with the PRD.
- **What would close it** (follow-up PR: carry the role on the sandboxed path, then allow it):
  thread `session_role` through the sandboxed claude-cli start so `record_role_before_spawn` writes
  it into the jailed session's initial `.session.yaml` and resume re-applies the role's
  `--allowedTools`; then drop `sandbox` from the refused placements and add an acceptance case in
  `packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` that sets main up
  in a sandbox and asserts the listed session carries `session_role: project-home`. Remove the code
  `TODO` in `project_home_start_spec.rs` with it.
