# 2026-11-05 — The session pointer has one slot per worktree and no owner check

**Category:** Deferred from `#agent-tools` 15/19 (#821, headless-driver)
**Source:** `packages/tddy-agent-backend/src/turn_files.rs` (`TurnFiles::write_session_pointer`, `Drop`); `packages/tddy-tools/src/session_source.rs` (`resolve_session`); found by `/validate-changes` on #821

A headless workflow turn writes `<workdir>/.tddy/session.json` before it spawns the agent and removes
it when the turn ends; a `tddy-tools` the agent runs finds its session there, ahead of `TDDY_SOCKET`.
Two cases the pointer does not handle:

1. **Two turns in one worktree.** The pointer is written with replace semantics. A second turn in the
   same worktree overwrites the first one's pointer, and whichever ends first removes the file, so the
   other turn's `tddy-tools` loses its session. Today a session runs one workflow turn at a time and
   sessions do not share worktrees, so nothing reaches this; a feature that runs parallel turns in one
   worktree would.
2. **A pointer left behind.** A `tddy-coder` killed with `SIGKILL` never runs `TurnFiles::drop`, and
   the git-excluded pointer stays. Every later `tddy-tools` run under that worktree resolves the dead
   session from it — ahead of `TDDY_SOCKET` — and fails to connect rather than using the variable.

What would close it: write an owner into `SessionPointer` (the writer's pid, or a creation token), and
have `resolve_session` treat a pointer whose owner is gone, or whose socket does not exist, as
`UnreadablePointer` naming that cause; and either refuse a second pointer for a different socket while
one is held, or reference-count the pointer per worktree. Both change the pointer format that
`tddy-tools` reads, so they belong with whichever change next extends the pointer — the conversation
pointer in `2026-11-05-headless-conversation-tool-server-env.md` is the likely one.
