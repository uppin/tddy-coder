# 2026-10-24 — Agent credentials carry the agent principal

**Type:** Feature

`#coagents` 8/8 ([#727](https://github.com/uppin/tddy-coder/pull/727)). `mint_agent_session_token` mints through `SessionTokenSigner::mint_agent`, so every agent credential — the local coding agent's tool server, the split agent, the sandboxed-codebase and project-home agents — carries `TokenPrincipal::Agent`. See [tddy-github session tokens](../../../tddy-github/docs/session-token.md).
