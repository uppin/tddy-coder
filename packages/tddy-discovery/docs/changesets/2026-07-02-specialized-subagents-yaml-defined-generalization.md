# 2026-07-02 — Specialized subagents (YAML-defined) generalization

**Type:** Feature

new `agent_def.rs`: `SpecializedAgentDef`/`SubagentTool`, `load_agent_defs`/`resolve_agent_defs` (`<tddyhome>/agents/*.yaml`, malformed files skipped, user defs override the builtin `fastcontext` def), `builtin_fastcontext_def` (today's shipped defaults). `SubagentRegistry::from_defs` resolves any number of registered defs (not just the hardcoded `"fastcontext"` factory) into a new `SpecializedSubagentSession`: optional system-prompt seeding, tool-gating to the def's bound tools (unbound calls rejected, not executed), and EndTurn-on-plain-prose termination (vs. the legacy `FastContextSession`'s citation-only convention) — both session types share a `send_turn_and_check_final_answer` turn-loop prefix helper, removing a ~30-line duplication. Feature [specialized-subagents.md](../../../../docs/ft/coder/specialized-subagents.md). (tddy-discovery, tddy-tools, tddy-coder, tddy-daemon, tddy-web)
