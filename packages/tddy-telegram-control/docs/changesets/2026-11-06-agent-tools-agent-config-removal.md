# 2026-11-06 — A Telegram delete removes an agent session's worktree again

**Type:** Fix

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `handle_delete_session` passes the controller's coding-agent registry to `delete_session_directory`, honouring `KeepWorktreeOnDelete`.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
