# 2026-07-02 — Subagent-declared tool replacement

- A discovery subagent (FastContext replaces Grep/Glob) can now declare which exec tools it takes over from the main agent — those tools are removed from the sandboxed Claude CLI's allowlist (not just discouraged in the prompt), and the managed-codebase appendix names the subagent that must be used instead
- New `--subagent-replaces <csv>` flag on `tddy-sandbox-app` (default: the subagent's own declared set)
- Daemon-hosted sandboxed sessions — previously with no discovery-subagent support at all — gain full parity with the standalone `tddy-sandbox-app` CLI, for both new-session start and resume
- Feature: [managed-codebase-subagents.md § Tool replacement](../managed-codebase-subagents.md#tool-replacement-subagent-declared)
