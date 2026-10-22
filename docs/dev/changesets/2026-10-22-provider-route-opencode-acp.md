# 2026-10-22 — OpenCode as an ACP tool-session agent

**Type:** Feature

`#provider-route` 7/9 ([#668](https://github.com/uppin/tddy-coder/pull/668)), across
`tddy-agent-backend` (`OpenCodeAcpBackend` in `backend/opencode_acp.rs`, `AnyBackend::OpenCode`, its
route in `backend/routed.rs`, its rows in every backend list, `model_catalog::build_backend`, and
`backend/acp_updates.rs` — the `session/update` and permission handling the three ACP clients now
share), `tddy-coder` (`--opencode-cli-path` / `TDDY_OPENCODE_CLI` / `opencode_cli_path`, the
`create_backend` arm, `"opencode"` in `BUILTIN_BACKEND_AGENT_IDS`), `tddy-tools`
(`list-models --opencode-cli-path` names the `opencode` agent's binary too), `tddy-daemon-rpc` (the
`opencode` model probe resolves the `opencode-cli` harness's binary), `tddy-acp-stub` (advertises
`stub-model` in every new session) and the dev / desktop configs (`allowed_agents`). Whether
`opencode acp` takes `--model` was not checked against a real OpenCode, so the route's `--model` is not
passed; the generated configuration selects the model. New backlog entry:
`2026-10-22-acp-backends-share-one-worker` (`TODO(opencode-acp)`). Code issues:
`oversized-file-codex-acp` closed (548 → 480 production lines, the shared client);
`oversized-file-backend-mod` 882 → 906 and `oversized-file-run` 2,598 → 2,620 (stack-shared, split
deferred). Feature: [OpenCode ACP backend](../../ft/coder/opencode-acp-backend.md).
