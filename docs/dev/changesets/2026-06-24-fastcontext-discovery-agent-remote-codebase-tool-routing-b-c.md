# 2026-06-24 — FastContext Discovery agent + remote codebase tool routing (B/C/D)

**Type:** Feature

new `packages/tddy-discovery` crate: `FastContextBackend: CodingBackend` (multi-turn `/v1/chat/completions` loop via reqwest), `ToolExecutor { Local | Remote }` (READ/GLOB/GREP against local fs or `ExecuteTool` RPC), `citation_lines_to_discovery_data` (maps `path:N-M` → `DiscoveryData.relevant_code`); `tddy-core` `RemoteToolEnv` gains envelope-construction helper; `tddy-coder`: `create_backend("fastcontext")` via `SharedBackend::from_arc`, `fastcontext_url`/`max_turns` config, `--agent fastcontext` CLI arg, `dev.daemon.yaml allowed_agents`. Feature [discovery-agent.md](../ft/coder/discovery-agent.md). Depends on `tddy-graph` extraction changeset. (tddy-discovery, tddy-core, tddy-coder)
