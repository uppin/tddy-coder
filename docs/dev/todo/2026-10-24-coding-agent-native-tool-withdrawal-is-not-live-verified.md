# 2026-10-24 — A managed coding agent's native-tool withdrawal is not verified against the real adapters

**Category:** Open risk, from `#coagents` 2/8 (#721)
**Source:** `packages/tddy-coding-subagent/src/confinement.rs` (`withdrawal_session_meta`, `opencode_withdrawal_config`)

In a managed-codebase session a coding agent must not touch the worktree except through
`tddy-tools --mcp`. Its native tools are withdrawn per harness, following each adapter's
documentation:

- `claude-acp`: `session/new` `_meta.claudeCode.options.disallowedTools` (Read, Write, Edit,
  MultiEdit, NotebookEdit, Bash, BashOutput, KillShell, Glob, Grep, LS);
- `opencode`: inline `OPENCODE_CONFIG_CONTENT` with `tools.{bash,read,write,edit,patch,glob,grep,list} = false`.

What is proven: the `_meta` reaches `session/new` (against `tddy-acp-stub`) and the opencode config
is in the launch environment (unit tests). What is **not**: that a live `claude-agent-acp` or OpenCode
honours them — a version that ignores the key would run with its native tools in a managed session.

## What would close it

A production test per harness that starts the real adapter in a managed launch, asks it to run a
native tool (`Bash`, `bash`), and asserts the tool is not offered or refused — gated like the other
harness-backed tests (an installed binary and credentials). If either adapter cannot be confined,
its `confinement_for` row becomes `CannotWithdraw`, which refuses it in managed sessions.
