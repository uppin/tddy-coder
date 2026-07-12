# Changesets Applied

Wrapped changeset history for tddy-tool-engine.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-12** [Feature] **New shared `tddy-tool-engine` crate** — generic tool-dispatch engine extracted from `tddy-daemon` (`tool_engine.rs` + `tool_catalog.rs`) so the daemon and coder share one implementation. Public API: `execute_tool(worktree_root, tool_name, args_json, &registry, session_id)`, `execute_tool_with_env(...)`, `ToolOutcome`, `ToolDef { name, description, input_schema_json }`, `tool_catalog()`. Tools: `Read`/`Write`/`StrReplace`/`Delete`/`Grep`/`Glob`/`Shell`/`Await`/`ReadLints`/`SemanticSearch`, all path-contained against `worktree_root`; background `Shell` jobs via `tddy_task::TaskRegistry`. Deps: `tddy-task`, `glob`, `bytes`, `serde_json`, `tokio`, `async-trait`, `log`. Tests: `tests/execute_tool_acceptance.rs` (Write→Read, path-traversal rejection, unknown-tool honest error, catalog completeness) + `catalog::tests`. Reference [README](../README.md). Cross-package [docs/dev/changesets.md](../../../docs/dev/changesets.md). PR [#297](https://github.com/uppin/tddy-coder/pull/297). (tddy-tool-engine)
