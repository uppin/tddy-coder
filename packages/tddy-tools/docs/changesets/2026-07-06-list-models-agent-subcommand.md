# 2026-07-06 — `list-models --agent` subcommand

**Type:** Feature

new `list_models.rs` `run_list_models(agent)` builds the backend for `<agent>` (same binary-resolution rules as `tddy-coder`, optional `--cursor-cli-path`/`--codex-acp-cli-path`… overrides via `cli.rs` `ListModelsArgs`), calls `tddy_core::CodingBackend::list_models()`, and prints the daemon⇄tools JSON contract `{ "models": [{"id","label"}], "default_model": "<id>" }` on stdout (`--agent claude-cli` → curated Claude full-id catalog). Registered as the `ListModels` subcommand in `main.rs`/`cli.rs`. Tests: `render_models_json` contract 1 — suite 29/29. Feature [tool-session-model-selection.md](../../../../docs/ft/web/tool-session-model-selection.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-tools)
