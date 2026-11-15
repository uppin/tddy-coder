# 2026-11-15 — The project-home tools can inspect and drive a split session

The project-home tools act on a split session by its agent half's id: the tools
that act on the agent (terminal, activity, transcript, messages, answers,
terminate, delete, marks) run where the agent runs, and the worktree and git tools
act on its codebase half on the codebase host and answer with that session's id.
A session with neither a worktree nor a codebase half is still refused by the
worktree tools. See [project-concept.md § Project-home tools](../project-concept.md#project-home-tools).
