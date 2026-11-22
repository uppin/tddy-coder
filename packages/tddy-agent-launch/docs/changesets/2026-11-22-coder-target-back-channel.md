# 2026-11-22 — A target child's reports, colours, branch, attachments and answered questions

**Type:** Fix · PR #997 · Cross-package: [docs/dev/changesets/2026-11-22-coder-target-back-channel.md](../../../../docs/dev/changesets/2026-11-22-coder-target-back-channel.md)

- `back_channel`: a `HookLog` command is `remote_hook_command` under `sh -c` (zsh read the piped report as empty).
- `remote_hook_follower`: a dropped line is logged with its reason.
- `remote_agent_launch::target_login_exec`: `env -i` names `tddy_pty::terminal_variables::TERMINAL_VARIABLES`.
- `cli_session_worktree::remote_worktree_through`: records the cut branch in the changeset, and copies attachments (`remote_attachments::copy_attachments_to_target`, new).
- `remote_hook_answer`: `HeldHook.event`; a `PermissionRequest` of `AskUserQuestion` is answered with `question_decision_json_for`.
