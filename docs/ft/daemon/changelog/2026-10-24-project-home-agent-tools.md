# 2026-10-24 — A project's main reaches its tools, and has no shell

A project's main (the project-home claude-cli session) is launched and resumed with `tddy-tools` as
its only MCP server, reaching this daemon as the main under a credential minted for it — over the
agent-tool socket on Tddy Desktop, over the local URL on a `tddy-daemon` — so `ListProjectSessions`,
`StartProjectSession`, `ReadSessionWorktree`, `DiffSessionWorktree` and the git tools are callable
from its chat. Claude's native filesystem and shell tools are withdrawn, so the main is read-only on
every route, and a role prompt tells it what it is and which tools to use. See
[project concept § Project-home session](../project-concept.md#project-home-session).
