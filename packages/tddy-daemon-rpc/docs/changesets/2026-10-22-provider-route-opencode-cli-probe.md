# 2026-10-22 — OpenCode model probe binary

**Type:** Feature

`#provider-route` 3/9 ([#664](https://github.com/uppin/tddy-coder/pull/664)). `ListAgentModels("opencode-cli")` resolves the binary with `cli_harness_binary` and forwards
`--opencode-cli-path` to `tddy-tools list-models` when it is absolute, as for Cursor's
`--cursor-cli-path` (`list_models_probe_args(agent, binary_path)`). See
[tool-session model selection](../../../../docs/ft/web/tool-session-model-selection.md).
