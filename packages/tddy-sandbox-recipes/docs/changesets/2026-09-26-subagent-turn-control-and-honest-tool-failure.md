# 2026-09-26 — `subagent_resume` in the sandboxed-Claude allowlist

**Type:** Fix

`SUBAGENT_TOOLS` gains `mcp__tddy-tools__subagent_resume`. The allowlist is hand-maintained and
separate from the MCP advertisement, so a tool reaching only one of them is advertised and
uncallable — which reads to the main agent as an agent that is not registered rather than as a tool
it may not use.

Pinned rather than merely added: `claude_allowlist_offers_subagent_resume_exactly_where_it_offers_subagent_prompt`
asserts the two appear and disappear together, so the next `subagent_*` tool cannot land on one
list alone.

[docs/ft/coder/managed-codebase-subagents.md](../../../../docs/ft/coder/managed-codebase-subagents.md)
· [../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
