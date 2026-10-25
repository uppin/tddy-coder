# 2026-10-25 — Headless `cursor`, `codex` and `oz` subagents cannot withdraw their native tools

**Category:** Deferred from `#coagents` 7/8 (#726, `headless-driver`) — developer-decided known limit; narrowed by `#coagents` 9/9 (#776, sweep)
**Source:** `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` (`confinement_for`, which reports all three `CannotWithdraw`)

#776 handed the three CLIs the `tddy-tools --mcp` server in each one's own shape (cursor's
`<worktree>/.cursor/mcp.json`, `oz --mcp <path>`, codex `-c mcp_servers.*` overrides), so they now
have the session's tddy-tools tools. What is left is the half #776 did not take: none of the three
is proven to have a mode in which its **native** tools (shell, file edits) are withdrawn, so a
managed session still refuses all three at attach, and an unmanaged one runs them under the fixed
policy their row names. None of them has a permission-prompt tool either, so none asks mid-turn.

Deferred because each CLI needs its own investigation against an installed binary (no `codex` binary
is installed on the development machine — only the Superset shim), and a withdrawal claim that is not
proven against the real CLI would let a managed session run unconfined tools.

## What would close it

Per CLI, find and prove (against the installed binary, recorded) a flag or config that removes its
native tools while keeping MCP tools; then move that harness's `confinement_for` from
`CannotWithdraw` to the withdrawal it supports, with a test that the managed attach admits it.
