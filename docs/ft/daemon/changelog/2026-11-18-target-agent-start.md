# 2026-11-18 — Claude and Cursor start on a target with its own binary, env, chat and trust

A session started on a target (an SSH host or a Coder workspace) runs the target's own agent with
the target's `HOME`, `PATH` and `TMPDIR`; a Cursor chat is minted on the target; and neither agent
stops at a first-run trust or MCP dialog: Claude's clone root is pre-trusted with project MCP servers
rejected, and Cursor starts with `--trust`. Local starts are unchanged.
