# 2026-11-07 — Docs and comments still name the deleted `ClaudeAcpBackend` / `CodexAcpBackend`

**Category:** Cleanup (stale documentation)
**Source:** `#agent-tools` 24/28 (`dead-launch-code-removal`, #832). That node fixed the two code comments
that named `ClaudeAcpBackend` (`tddy-acp-stub/src/main.rs`, `tddy-integration-tests`'s
`acp_backend_acceptance.rs`) and left these, which are not code. Owner: the `#agent-tools` sweep node.

## What is there

Both bespoke backends are gone (`SpecAcpBackend` is the ACP driver). These still describe them:

| File | Line | Says |
|---|---|---|
| `packages/tddy-acp-stub/README.md` | 3 | "testing ClaudeAcpBackend without real Claude" |
| `packages/tddy-acp-stub/README.md` | 23 | "ClaudeAcpBackend architecture" |
| `packages/tddy-acp-stub/Cargo.toml` | 5 | `description = "… testing ClaudeAcpBackend …"` |
| `packages/tddy-coding-agent-specs/specs/claude-acp.yaml` | 5 | "what `claude_agent_acp_command` and `ClaudeAcpBackend` build by hand" |
| `packages/tddy-coding-agent-specs/specs/codex-acp.yaml` | 10 | "what the launcher and `CodexAcpBackend` build by hand" |
| `packages/tddy-agent-backend/src/backend/codex_acp.rs` | 2 | "The bespoke `CodexAcpBackend` that owned them is …" |

`packages/tddy-agent-backend/docs/architecture.md` (lines 67 and 133) also names `ClaudeAcpBackend::with_command`,
`CodexAcpBackend::with_launch` and `tddy_coder::acp_from_spec` building them; `acp_from_spec` now only
resolves an `AcpLaunch` (the three `*_backend_from_spec` builders are deleted).

## What to decide

Reword each to the spec-driven launch. No behaviour changes.
