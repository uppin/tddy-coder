# 2026-11-21 — `coding_agent.peer_name`; `codex-cli` takes its first prompt

**Type:** Feature · `#orchestra` 2/9 (PR #980) · Cross-package: [docs/dev/changesets/2026-11-21-child-back-channel.md](../../../../docs/dev/changesets/2026-11-21-child-back-channel.md)

- `PeerNameSlot` / `CodingAgentSpec::peer_name`; `claude.yaml` declares `peer_name: { flag: --name }`.
- `codex.yaml`'s run argv passes the first prompt as the positional `[PROMPT]`.
- Agent variants (`AgentVariant`, `VariantError`, `HOST_AGENT_VARIANT`, `variant_args`,
  `variant_hooks_args`) moved to `spec/variant.rs`, still exported from the crate root. Closed
  `oversized-file-spec` (500 at detection, 519 at its peak here, 431 at close).
