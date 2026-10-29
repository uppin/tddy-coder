# 2026-10-28 — Agent context keeps the repository's symlinks

A managed-codebase agent's working directory now holds the repository's symlinks as symlinks, so a
`.claude/skills -> ../.agents/skills` layout gives a split agent its skills, commands and subagents.
A link may bring in a target no glob names — one hop, never what git ignores or anything
secret-shaped — and `CLAUDE.md -> AGENTS.md` still arrives as a file led by the managed-codebase
notice. See [Agent context sync § Design](../agent-context-sync.md#design) (AC19–AC19d).
