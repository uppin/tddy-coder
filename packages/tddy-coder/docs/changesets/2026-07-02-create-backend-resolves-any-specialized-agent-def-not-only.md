# 2026-07-02 — `create_backend` resolves any specialized-agent def, not only `"fastcontext"`

**Type:** Feature

`create_backend` now checks a resolved `Vec<SpecializedAgentDef>` (builtin + `<tddyhome>/agents/*.yaml`, via the new `resolve_specialized_agent_defs` helper, wired at all 5 call sites) for a matching agent name before falling back to the hardcoded `"fastcontext"` branch, building `FastContextBackend` from the def's model/base_url/max_turns; explicit `--fastcontext-*` CLI flags still take precedence over the resolved def. `--agent`'s clap allowlist doesn't yet recognize custom names (tracked in `docs/dev/TODO.md`). Feature [specialized-subagents.md](../../../../docs/ft/coder/specialized-subagents.md). (tddy-coder, tddy-discovery)
