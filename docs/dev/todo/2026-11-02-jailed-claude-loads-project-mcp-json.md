# 2026-11-02 — A jailed Claude loads the project's `.mcp.json` beside `tddy-tools`

**Category:** Decision for the developer
**Source:** `#agent-tools` 5/12 (#794), `TODO(claude-jail)` at
`packages/tddy-agent-launch/src/claude_jail.rs` (`fn jailed`)

## What is there

`claude.yaml`'s MCP slot declares `strict_flag: --strict-mcp-config`, and an unsandboxed main passes
it, so it loads only the servers named by `--mcp-config`. The jail renders the same slot **without**
the strict flag (`jailed()` clears it): the jail's context directory holds the project's own
`.mcp.json`, and the hand-built jail launch #794 replaced never passed `--strict-mcp-config` either,
so the jailed agent has always loaded the project's servers beside `tddy-tools`.

## Why it was not changed in #794

Placing the flag is not the obstacle — it could go before `--mcp-config` so the argv still ends with
the registration path, as `sandboxed_claude_launch_baseline` and
`claude_cli_jail_from_spec_acceptance` require. Passing it at all is a behaviour change: a project's
own MCP servers would disappear from every sandboxed session. #794 is coverage-first and keeps
today's behaviour.

## What would close it

Decide whether a jailed agent should load only `tddy-tools`, as an unsandboxed main does.

- **Strict:** drop `jailed()`, and render the strict flag ahead of the path. That needs the slot
  renderer (#791, `slots/mcp.rs`, which pins the flag *after* the path) to take a placement, or
  the jail to reorder it. Add a test that a project `.mcp.json` server is not loaded.
- **Not strict:** remove the `TODO(claude-jail)` and record the choice in
  `docs/ft/daemon/claude-cli-session.md` § Sandbox mode.
