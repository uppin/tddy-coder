# 2026-08-18 — **session agent roster

**Type:** Feature

every hardcoded builtin agent is deleted** — `builtin_fastcontext_def` / `builtin_agent_defs` are removed; `subagent_replaced_tools` (its hardcoded `"fastcontext"` arm) and `resolve_replaced_tools` (the CSV override) are deleted, so `replaces` is a plain union computed only from the defs given. `FastContextBackend` is renamed `SpecializedAgentBackend` and its model/base URL/turn budget come only from the def. An empty agents directory resolves to nothing. Feature [session-agent-roster.md](../../../../docs/ft/daemon/session-agent-roster.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-discovery)
