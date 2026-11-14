# 2026-11-12 — Any number of orchestrator sessions beside a project's main

A project can run **orchestrator sessions**: read-only sessions on its home worktree with main's reach.

- Start one from New session with the **Session orchestrator** switch (agents that declare `project_home_main`), or from main or another orchestrator through `StartSession` with `session_role: orchestrator`.
- The daemon places it as it places main and refuses a placement, a sandbox, or an agent that cannot be held read-only, naming the field.
- It gets main's tools and main's read-only refusal. Git tools will not write to another orchestrator's worktree.
- Only main is auto-stopped when idle. Orchestrators carry an **orchestrator** badge in the session drawer.
- A session an orchestrator starts records its orchestrator before its agent runs.
- An agent may ask for the role only when its own session holds one. The daemon reads that session from the agent's token, never from the request.

See [Project concept § Orchestrator sessions](../project-concept.md#orchestrator-sessions).
