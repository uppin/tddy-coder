# 2026-07-01 — Dynamic tool catalog wired into the live MCP server

**Type:** Fix

`#[tool_handler(router = self.tool_router)]` fixes the macro's default expansion calling the static `Self::tool_router()` instead of the instance field; `exec_tool_catalog()`/`dynamic_tool_router(catalog)`/`tool_names()` now expose all 10 documented cursor tools over `tools/list`/`tools/call` (previously only the 3 static `#[tool]` methods were visible). Feature [remote-codebase-mode.md](../../../../docs/ft/daemon/remote-codebase-mode.md) (AC15-19). (tddy-tools)
