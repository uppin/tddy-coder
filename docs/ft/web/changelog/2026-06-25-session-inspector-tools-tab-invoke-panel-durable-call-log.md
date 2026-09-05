# 2026-06-25 — Session inspector Tools tab: invoke panel + durable call log

- Session inspector drawer gains a **Details / Tools** tab strip; Details tab is selected by default (existing metadata/controls unchanged)
- **Invoke panel**: tool picker (`ListExecTools`), JSON args textarea seeded from tool's JSON Schema (`defaultArgsFromSchema`), Invoke button calls `ExecuteTool`; result renders in a code block; errors show a styled error box
- **Call log**: collapsible rows newest-first from new `ListSessionToolCalls` RPC; each row shows tool name + status; expanded row shows Input (`args_json`), Output (`result_json`), and stdio panels (Shell rows parse `stdout`/`stderr`/`exit_code`)
- **Durable persistence**: every `ExecuteTool` invocation is appended to `~/.tddy/sessions/{id}/tool-calls.jsonl`; the log survives daemon restarts and in-memory registry eviction
- Log is scoped per session and capped at 500 most-recent entries; empty state message when no calls recorded
- After a successful invoke the call log automatically refetches to show the new row
- Feature: [session-drawer.md](../session-drawer.md)
