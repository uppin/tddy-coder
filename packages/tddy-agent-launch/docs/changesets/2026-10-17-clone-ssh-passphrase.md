# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

`ClonePlanner` / `PlannedClone`, `CloneBackend`, `clone_ssh_env` (`ClonePlan`, `clone_ssh_env_for`, `CloneAgentError`) and `start_keepalive::with_keepalive`; `LaunchSessions.askpass`. A clone runs with the askpass environment, or without prompting when OpenSSH is older than 8.4 or no agent is reachable. A forwarded `StartSession` waits `start_forward_deadline` (the split-start budget) and a streamed start sends a keep-alive every 5 s while it provisions. `ensure_project_available_for_start` stays at 99 lines and `spawn_project_clone` shrinks from 86 lines at nesting 7 to 57 at nesting 3. See [the module layout](../module-layout.md#cloning-a-project-for-a-start).
