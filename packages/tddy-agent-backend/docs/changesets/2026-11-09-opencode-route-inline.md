# 2026-11-09 — The `inline_config` route shape (#agent-tools 29/32, #857)

- `RouteShape::InlineConfig` (YAML `inline_config`), `RouteShape::word()`; `bodies_accept(InlineConfig)` = OpenAI Responses, OpenAI Chat Completions.
- `RoutedAgent.config_env`; `inline_config_route` (env `[(config_env, <config JSON>)]`, secret `key_env`, no files, `--model tddy-route/<model>`), `NotYetRoutable` without `key_env` or `config_env`.
- `adapters/opencode_route_file.rs` and its `opencode-route-file` registration deleted.
