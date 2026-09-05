# 2026-08-14 — Choosing which daemon holds a session's codebase

- **A "Codebase host" selector in the new-session form.** Inside the claude-cli managed-codebase block, it places the session's git worktree on a daemon other than the one running the agent — defaulting to "Same as host", which is exactly today's behaviour. See [remote-managed-worktree.md](../../daemon/remote-managed-worktree.md).
- **Choosing a codebase host withdraws what a split session cannot have** — the workflow recipe, the sandbox, the semantic index and `--dangerously-skip-permissions`. The daemon refuses each by name, and the form defaults the recipe to a non-empty value, so without this the *only* thing the selector could produce was a request that could not succeed.
- **Naming the session's own host means co-located**, matching how the daemon classifies it, rather than reading as a split and silently launching the session without its workflow.
- **Not offered where it cannot apply:** cursor-cli sessions (which cannot enforce a split), peer spawns (whose worktree is the orchestrator's, already placed), and installs with no other daemon in the common room.
- **The sessions drawer badges a split session's codebase host**, resolved through the same label mapping as the agent host, and only for rows that actually have one.
