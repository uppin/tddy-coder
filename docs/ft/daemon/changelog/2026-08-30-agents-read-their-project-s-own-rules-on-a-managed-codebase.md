# 2026-08-30 — Agents read their project's own rules on a managed codebase

- **A managed session's agent now gets the target repo's own guidance**, chosen by a per-backend allow-list of globs rather than the fixed, project-specific list it used before — so a Cursor session receives `.cursor/`, and no session receives this repository's `docs/` and `skills/` conventions.
- **A split session gets it at all.** Its agent works on a host with no repository, and its context directory previously held only the managed-codebase notice; it is now populated from the codebase daemon before the agent starts, on start and on resume.
- **The managed-codebase notice moved to the top** of `CLAUDE.md` and `AGENTS.md`, so the rule that the codebase lives elsewhere is read before the project's own instructions, and it names which paths the sync owns and will replace.
- **A failed guidance fetch fails the session** rather than starting an agent that silently has no project rules.
- ⚠️ **Not yet continuous.** The directory is built at start and resume and is not updated again for the life of the session; the re-sync trigger is unwired and tracked in [docs/dev/TODO.md](../../../dev/TODO.md).
- ⚠️ **Both halves of a split placement must be upgraded together** — start and resume now make two mandatory peer calls that an older codebase daemon cannot answer.
