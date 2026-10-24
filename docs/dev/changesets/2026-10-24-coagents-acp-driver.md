# 2026-10-24 — One ACP client driver for every ACP coding backend

`#coagents` 1/8 ([#720](https://github.com/uppin/tddy-coder/pull/720)). Packages: `tddy-agent-backend`,
`tddy-acp-stub`.

The three copies of the ACP worker (`claude-acp`, `codex-acp`, `opencode`) are replaced by one
`AcpDriver` that can set the session's model, cancel a turn, pass MCP servers, route permission
requests to a handler and kill its child on drop. Resolves
`2026-10-22-acp-backends-share-one-worker`; closes `complexity-acp-run-acp-worker` and
`complexity-codex-acp-run-codex-acp-worker`. Details:
[tddy-agent-backend changeset](../../../packages/tddy-agent-backend/docs/changesets/2026-10-24-coagents-acp-driver.md).

**Open risk (needs a live check before merge):** the driver refuses a requested model the agent does
not advertise, and the workflows send `claude-acp` the default model `opus`. If the real
`claude-agent-acp` does not advertise `opus` verbatim, every `claude-acp` run fails at session open.
