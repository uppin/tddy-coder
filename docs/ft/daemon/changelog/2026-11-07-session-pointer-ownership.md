# 2026-11-07 — A session pointer is owned, and cleaned up over SSH too

`#agent-tools` 21/28 ([#829](https://github.com/uppin/tddy-coder/pull/829)).

- **A stale pointer fails loudly.** `.tddy/session.json` names its owner; a `tddy-tools` on the
  owner's host that finds the owner gone, or the socket it names missing, fails with an error naming
  the pointer instead of dialling a dead session. A pointer from an older daemon (no owner) still
  resolves; `--socket` is never checked.
- **One pointer per worktree is held.** A turn for another session is refused rather than
  overwriting a pointer whose owner is alive.
- **SSH checkouts match local ones.** The pointer on an ssh target is added to the target checkout's
  `info/exclude`, written `0600`, and removed when the agent exits and when the session is deleted,
  each with one bounded ssh command — it no longer shows as an untracked file there.
- **Headless Codex conversations carry no `TDDY_*` env.** Their tool server finds the conversation
  through a pointer in the conversation directory, started there by a `cwd` override.
- **Concurrent jails get distinct tool-IPC sockets**, even for sessions started in the same
  millisecond.

See [Claude CLI session](../claude-cli-session.md) and [Session agent roster](../session-agent-roster.md).
