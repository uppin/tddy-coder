# Changesets Applied

Wrapped changeset history for tddy-discovery.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-24** [Feature] **Initial implementation — FastContext discovery agent** — `FastContextBackend: CodingBackend` (multi-turn OpenAI loop via `reqwest`, `microsoft/FastContext-1.0-4B-RL`, READ/GLOB/GREP tools, `<final_answer>` extraction); `ToolExecutor { Local | Remote(RemoteToolEnv) }` (local: `std::fs`/`glob`/`regex`; remote: `ExecuteTool` RPC POST, `is_error` surfaced without fallback); `ChatMessage` named constructors (`user`/`system`/`assistant`/`tool_result`); `citation_lines_to_discovery_data` (`path:N-M` → `DiscoveryData.relevant_code`, malformed excluded); `extract_final_answer` (tag-based extraction). Feature [discovery-agent.md](../../../docs/ft/coder/discovery-agent.md). (tddy-discovery, tddy-coder)
