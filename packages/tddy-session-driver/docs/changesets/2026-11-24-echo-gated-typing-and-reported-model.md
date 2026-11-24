# 2026-11-24 — `send_keys` presses keys after the text's echo

**Type:** Fix

`send_keys` encodes with `encode_typing` and types through `tddy_tool_engine::terminal_keys::type_into`:
`DriverSession` implements `TypingTerminal` (offset from its grid, writes through `send`). Keys after
text are written once the agent has echoed the text, or after 5 s with no echo; the answer is
`Typed::answer()` — `{sent_bytes}`, plus `echo_seen` when keys followed text. Live against Cursor
Agent 2026.11.22 (local pty, local tmux, `coder ssh` → remote tmux) one `send_keys` of text + Enter
submitted every time.

Doc: [architecture.md § The MCP tools](../architecture.md#the-mcp-tools). Full record: [cross-package entry](../../../../docs/dev/changesets/2026-11-24-cursor-tui-submit-and-resume-model.md).
