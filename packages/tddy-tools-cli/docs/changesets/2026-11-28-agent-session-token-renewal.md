# 2026-11-28 — tddy-tools --mcp keeps its session token alive

**Type:** Feature · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)

`run_mcp_server` spawns `tddy_session_tool_client::session_token::spawn_session_token_renewal`
beside its tool server, so the agent's 24 h session token is renewed over its own daemon before it
runs out — an agent that outlives one minting keeps its tools, where an orchestrator (no shell)
previously lost every one of them at expiry.
