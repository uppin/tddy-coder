# 2026-11-05 — A Codex conversation's tool-server variables are still `TDDY_*` in its environment

**Category:** Deferred from `#agent-tools` 15/19 (#821, headless-driver)
**Source:** `packages/tddy-agent-backend/src/backend/headless_driver.rs` (`HeadlessCommand::env`, marked `TODO(headless-driver)`); `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` (`headless_plan_from_spec`, `forwards_server_env`)

The stack rule is that a headless agent's process sees no `TDDY_*` variable. #821 applies it to the
workflow backend (`SpecHeadlessBackend`) completely, and to `HeadlessDriver` (conversations) for
everything the driver inherits: it strips every inherited `TDDY_*` variable before it spawns.

What it cannot strip is what the launcher puts in the command on purpose. A Codex conversation's MCP
slot is inline (`-c mcp_servers.tddy-tools.env_vars=[…]`), which **names** the server's variables and
leaves their values to Codex's own environment, so the credential stays off the argv. The launcher
therefore adds `server.env` (`TDDY_REMOTE_SESSION_TOKEN`, `TDDY_CONVERSATION_ID`, …) to
`HeadlessCommand::env`, and `HeadlessDriver` hands it to the child as asked.

To close it, a conversation needs a session pointer of its own — a `.tddy/session.json` in its
worktree carrying what `tddy-tools` needs (the session's socket, the conversation id and the minted
credential, through the same 0600 file the workflow pointer is) — so the inline slot names no
variables and the launcher puts nothing in `env`. That changes what the pointer carries
(`conversation_id`, a token), which is the session-pointer owner's decision, not this stack's.
