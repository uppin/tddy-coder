# 2026-07-02 — Subagent-declared tool replacement

**Type:** Feature

`subagent_replaced_tools(name)` / `resolve_replaced_tools(name, override_csv)`: a subagent (FastContext → `Grep`+`Glob`) declares which exec tools it replaces on the main agent; override wins outright over the default, tokens normalized to canonical casing, unrecognized tokens dropped. Feature [managed-codebase-subagents.md § Tool replacement](../../../../docs/ft/coder/managed-codebase-subagents.md#tool-replacement-subagent-declared). (tddy-discovery, tddy-sandbox, tddy-sandbox-recipes, tddy-sandbox-runner, tddy-sandbox-app, tddy-daemon)
