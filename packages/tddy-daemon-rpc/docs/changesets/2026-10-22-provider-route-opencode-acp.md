# 2026-10-22 — The `opencode` agent's model probe

**Type:** Feature

`#provider-route` 7/9 ([#668](https://github.com/uppin/tddy-coder/pull/668)). `ListAgentModels` for
the `opencode` agent resolves the binary of the `opencode-cli` harness (`listing_harness_session`) and
`list_models_probe_args` passes it as `--opencode-cli-path`, so the probe runs the same `opencode` a
terminal session would.
