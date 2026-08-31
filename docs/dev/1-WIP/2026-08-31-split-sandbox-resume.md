# Changeset: split-sandbox-resume

State A → State B amendment: `docs/ft/daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md` (resume/delete criteria).
PRD (target): `docs/ft/daemon/remote-managed-worktree.md`, `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox.

## Responsibility

- Workspace resume re-provisions the sandbox jail from persisted `sandbox: Some(true)` and worktree path.
- `DeleteSession` tears down sandbox resources on the workspace half (orphaned jail runner, `<session_dir>/sandbox` tree) alongside worktree/session-dir removal in `session_deletion.rs`.
- Split agent resume unchanged except a regression test proving no sandboxed-agent resume regression (agent metadata stays `sandbox: None`).
- Resume/delete acceptance tests.

## Boundaries

- Does **not** implement initial workspace sandbox provision at start — consumes `workspace-tool-sandbox` (#427).
- Does **not** remove the split+sandbox refusal or cross-host happy-path start — consumes `split-sandbox-orchestration`.
- Does **not** touch web UI.
- Does **not** add new proto fields or change co-located resume paths.

## Dependencies

- **`workspace-tool-sandbox`** (#427, landed): delivers the `WorkspaceSandbox` / `WorkspaceSandboxProvisioner` provision/execute/teardown API used at start; this PR adds resume-time re-provision and delete-time teardown calling the same API.
- **`split-sandbox-orchestration`**: delivers working split+sandbox start and the metadata contract (agent `sandbox: None`, workspace `sandbox: Some(true)`); this PR assumes that pairing exists and tests resume after a sandboxed workspace session was created.

## Draft PR contract

Land first:

1. `teardown_workspace_sandbox(session_dir)` hook wired into `session_deletion.rs` (failing test: delete leaves sandbox runner process running today).
2. Resume workspace path calls reprovision when `meta.sandbox == Some(true)` (failing test: resumed workspace executes tools on host FS today).
3. Regression test: split agent resume with `sandbox: None` metadata never calls `resume_sandboxed_claude_cli_session`.

Implementation of full cross-host resume can follow in the same PR once the hooks and failing tests land.

## TODO

- [x] Create changeset — this document
- [x] Failing acceptance tests (Step 6 — `/plan-red`)
  - `tests/workspace_sandbox_resume_acceptance.rs`: resume re-provisions the jail after a restart; resume reuses the live jail when not restarted; delete after a restart kills the orphaned jail runner.
  - `tests/split_session_resume_acceptance.rs`: a split session's agent half (`sandbox: None`) resumes through the split path, not the sandboxed runner path.
- [ ] Failing unit/integration tests (Step 7 — red phase)
- [x] Implement production code making tests pass (`/green`)
  - `src/connection_service.rs`: workspace branch in `resume_session` — re-provisions jail when `sandbox == Some(true)` and not already registered; returns empty LiveKit fields.
  - `src/session_deletion.rs`: `teardown_workspace_sandbox` reads `<session_dir>/sandbox/runner.pid` and terminates the orphaned runner before `remove_dir_all`; `reap_child_if_ours` reaps zombies so `kill(pid,0)` no longer lies.
  - `src/workspace_tool_sandbox.rs`: `RUNNER_PID_FILE` const; production provisioner persists `handle.pid()` to `<session_dir>/sandbox/runner.pid`.
  - Verified: 3/3 `workspace_sandbox_resume_acceptance`, 8/8 `split_session_resume_acceptance`, 15/15 `workspace_tool_sandbox_acceptance`, 2/2 `workspace_session_deletion_acceptance`, clippy clean. 2 cross-host `remote_managed_worktree_cross_host_acceptance` failures confirmed pre-existing on red-only baseline (sandboxed split-start refusal, owned by `split-sandbox-orchestration`).
- [ ] Prepend changeset index line to `docs/dev/changesets.md` (`/wrap-context-docs`)
