# 2026-03-10 — tddy-tools Relay Handler

**Type:** Feature

ToolExecutor trait (InMemoryToolExecutor for tests, ProcessToolExecutor for tddy-demo). StubBackend uses tool_executor. BackendInvokeTask prefers take_submit_result_for_goal over stream parsing. toolcall module: store_submit_result, take_submit_result_for_goal, ToolCallRequest, ToolCallResponse, socket listener. Presenter integration: tool_call_rx, poll_tool_calls. TDDY_SOCKET env var in Claude/Cursor backends. Bash(tddy-tools *) in allowlists. System prompts instruct tddy-tools submit/ask. Output parsers accept raw JSON first. (tddy-core)
