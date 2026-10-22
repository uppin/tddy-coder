# 2026-10-22 — a tool session that names no account does not use the person's default

**Category:** Deferred from `#provider-route` 5/9 `default-route` (#666)
**Source:** `/validate-changes` on #666

#666 feeds `RouteSources.user_default` (`session_route_sources_with_default` in
`packages/tddy-agent-launch/src/session_provider_route.rs`) into the claude-cli and opencode-cli
starts, and into the sandboxed claude-cli start's refusal. A **tool session** still decides its route
from the session-level choice alone, so a person whose default for `codex` is Meta gets the vendor
on a tool session that names no account.

**Why deferred:** the tool-session start is #665's (`#provider-route` 4/9, `tool-route`). While #666
was greened, #665's `tool_session_route_json` was a `todo!()` with no caller. After #666 was rebased
onto #665's green tip the call site exists (`tool_session_spawn.rs`, `session_route_sources(&req)`),
but wiring it is a product decision, not a one-liner:

- **Which id keys a tool session's default.** Defaults are stored per harness id, and the Accounts
  screen offers only `claude-cli` and `opencode-cli`. A tool session's harness is its agent id
  (`claude`, `claude-acp`, `codex`, …), so reading `default_for` under the agent id finds nothing
  the screen can set, and reading it under the session type would make a Claude CLI default silently
  apply to Claude tool sessions too.
- **Blast radius of the locked-vault refusal.** A start that names no account refuses while the
  vault is locked (no fallback to the vendor). Applied to tool sessions — the common session kind —
  every Claude/Codex tool session of a person with a locked vault would be refused.

The web pane already pre-selects a default for a tool session's agent when one is stored under that
agent id, and then sends it as an explicit choice, so nothing is silently run on the wrong account.

**Plan pointer:** decide the key (agent id vs. a harness family shared with the CLI type) and whether
tool sessions should refuse on a locked vault, then change `tool_session_spawn.rs` to
`session_route_sources_with_default(&req, <key>, &access)` (read only when the agent maps to a
`Harness`), add the matching options to `ProviderAccountDefaultsControl`, and pin it in
`tool_session_route_tests.rs`. Owner: the stack's sweep node or a follow-up PR.

## What would close it

A tool session started with no account choice resolves its route through `user_default`, pinned by
a test, and this entry is deleted.
