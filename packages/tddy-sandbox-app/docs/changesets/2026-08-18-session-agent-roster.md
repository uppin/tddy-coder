# 2026-08-18 — **session agent roster

**Type:** Feature

action-author/coder validation removed; no builtin fallback** — `validate_subagent_roles` (replacing `Shell` ⇒ action author, replacing `Write`/`StrReplace`/`Delete` ⇒ coder, must-bind) is deleted; `resolve_specialized_agents` resolves only against `<tddyhome>/agents`, with no builtin to fall back on. Feature [session-agent-roster.md](../../../../docs/ft/daemon/session-agent-roster.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox-app)
