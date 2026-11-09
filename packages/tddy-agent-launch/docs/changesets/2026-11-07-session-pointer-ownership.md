# 2026-11-07 — The ssh session pointer is excluded, owner-only and removed

**Type:** Feature

`#agent-tools` 21/28 ([#829](https://github.com/uppin/tddy-coder/pull/829)).

- `cli_session_pointer.rs`: the pointer is owned by the daemon; `PointerHost::{Local, Ssh { ssh, alias }}` and `PlacedSessionPointer { path, host }`. On an ssh target `place_session_pointer` runs one bounded command (`info/exclude` append unless present, `umask 077` write); `removing_session_pointer_on_exit` and `remove_session_pointer(worktree, host)` remove it there with a bounded `rm -f`, on a blocking thread. `record_remote_owner` / `REMOTE_AGENT_PID_FILE` are implemented and tested without a caller (`#[allow(dead_code)]`, [todo](../../../../docs/dev/todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md)). The `TODO(cli-dispatch-collapse)` on the ssh write is gone.
- `slot_file_over_ssh.rs`: `run_over_ssh(ssh, alias, path, remote, stdin, deadline) -> String` is the bounded ssh core the slot write and the pointer commands share; a stdout read error is its error.
- `svc_signal_delete_session.rs`: `remove_recorded_session_pointer(session_dir, ssh)` removes the pointer on the recorded `ssh_config_host` too.
- Tests: `cli_session_pointer/cli_session_pointer_unit_tests.rs` (a stub `ssh` that records and runs the remote command).

## Restructuring (engine-driven, `tddy-tools restructure move_item`)

| File | Production lines (to the first `#[cfg(test)]`) | Moved out |
|---|---|---|
| `cli_agent_steps.rs` | 652 → 468 | the conversation steps — `LaunchConversation`, `ConversationStart`, `start_conversation`, `resumed_conversation`, `resumed_or_adopted_conversation`, `conversation_mint_refused`, `refuse_a_role_the_agent_cannot_hold` — into `cli_agent_steps/conversation.rs`, callers re-pointed |
| `opencode_cli_spawn.rs` | 660 → 412 | the resume — the `LaunchSessions::resume_cli_harness` impl, `ResumedSession`, `refuse_a_resume_without_its_worktree`, `resume_cli_harness_session` — into `opencode_cli_spawn/resume.rs` |

`run_over_ssh` is 54 lines: `spawn_ssh`, `read_to_end_off_thread` and `wait_within` carry the spawn, the pipes and the deadline loop.

## Code issues

- **Closed** `oversized-file-cli-agent-steps` — 651 on `origin/master`, **468** after the split. Record deleted.
- **Closed** `oversized-file-opencode-cli-spawn` — 652 on `origin/master`, **412** after the split. Record deleted.
- Moved `complexity-opencode-cli-spawn-resume-cli-harness-session` → `complexity-opencode-cli-spawn-resume-resume-cli-harness-session` (115, unchanged) and `complexity-opencode-cli-spawn-resume-cli-harness` → `complexity-opencode-cli-spawn-resume-resume-cli-harness` (87, unchanged).
- `complexity-opencode-cli-spawn-start-cli-harness-session` unchanged at 300.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-07-session-pointer-ownership.md).
