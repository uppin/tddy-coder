# complexity: render

**Location:** `packages/tddy-coding-agent-specs/src/slots/mod.rs:101` — `render`
**Category:** complexity
**Detected:** 2026-11-03 — `/pr-wrap` on #797 (`#agent-tools` 8/12), fn line to closing brace on `origin/feature/agent-tools/cursor-cli` (`ca7814b57`) and HEAD; no coverage run
**Metrics:** **66 lines**
**Thresholds breached:** length 65 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — partially fixed (the hooks arm is extracted; the MCP arm extraction still closes it) — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-03 | 59 → 65 | #797 pushed it over: the `McpSlot::Unsupported` refusal before the MCP render (6 lines) |
| 2026-11-05 | 69 (unchanged) | touched by #820 (one line in the module, not in `render`); measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap |
| 2026-11-15 | 69 → 66 | **better by 3** in #911: the hooks arm moved into `render_hooks_slot` beside `render_tool_slot`; brace-matched on `origin/master` (`66bb45213`) and HEAD at #911's wrap |
| 2026-11-16 | 66 (unchanged) | touched by #920 (`#remote-shell` 6/7): three test literals in the module gained `remote_log: None`; `render` itself gained no branch — the carrier is a field of the params it passes through. Brace-matched at #920's wrap |

## What would close it

Move the MCP arm (the refusal plus `mcp::render_mcp`) into a `render_mcp_slot` helper beside `render_tool_slot`; `render` then only sequences the slots and is back under 60.
