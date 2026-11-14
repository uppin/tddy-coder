# 2026-11-13 — Orchestrators mark, pause and delete sessions and manage worktrees

An orchestrator keeps its ledger in the sessions themselves and cleans up after them.

- **`SetSessionMark`** records `paused`, `blocked` or `done`, with a note, in the session's
  `.session.yaml`; the mark is listed on `SessionEntry` and shown as a badge on the drawer row,
  the note on hover. `paused` also stops the session; any resume clears a `paused` mark.
- **`DeleteSession`** deletes a stopped session as the web's Delete does; a running session is
  refused.
- **`ClearWorktree`**, **`DeleteWorktree`** and **`RestoreWorktree`** do what the Worktrees
  screen's Clear, Delete and Restore do, refused while a session owning the worktree is active.
  `DeleteWorktree` keeps the branch and refuses the project's home worktree whoever calls;
  `RestoreWorktree` is addressed by `session_id`.
- All five tools reach a session or worktree on another host.

See [Project concept § Project-home tools](../project-concept.md#project-home-tools).
